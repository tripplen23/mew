//! `POST /webhook/github` — the @mewcli review bot endpoint.
//!
//! Thin route: verify the `X-Hub-Signature-256` HMAC, detect a `@mew`
//! mention, ack the delivery, and hand the work to
//! `services::github_bot` (which runs detached — an LLM review takes
//! longer than GitHub's delivery timeout, and a non-2xx would cause
//! GitHub to redeliver and double review).

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::AppState;
use crate::services::github_bot;

type HmacSha256 = Hmac<Sha256>;

/// `POST /webhook/github` — verify, accept, and fan out to a detached
/// review task.
///
/// Contract:
/// - incomplete GitHub App config → 404 (endpoint disabled)
/// - missing/bad `X-Hub-Signature-256` → 401 (GitHub flags the delivery)
/// - duplicate `X-GitHub-Delivery` → 200 with `accepted: false` (already
///   handled; a non-2xx would make GitHub redeliver and double-review)
/// - anything else → 200. Real failures are logged and the delivery acked
///   rather than replayed.
///
/// Not in the OpenAPI spec: utoipa cannot describe `Bytes` bodies, and
/// GitHub (not humans) is the only client of this endpoint.
pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    if !state.config.github.is_complete() {
        tracing::warn!("github webhook delivery ignored: app config incomplete");
        return (StatusCode::NOT_FOUND, Json(json!({ "accepted": false })));
    }
    let secret = state
        .config
        .github
        .webhook_secret
        .as_deref()
        .expect("is_complete ensures secret");
    let Some(signature) = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
    else {
        tracing::warn!("github webhook delivery rejected: missing signature");
        return (StatusCode::UNAUTHORIZED, Json(json!({ "accepted": false })));
    };
    if !verify_signature(secret, &body, signature) {
        tracing::warn!("github webhook delivery rejected: bad signature");
        return (StatusCode::UNAUTHORIZED, Json(json!({ "accepted": false })));
    }

    if let Some(delivery_id) = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
    {
        let mut seen = state.webhook_deliveries.lock().await;
        let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(600);
        seen.retain(|_, at| *at > cutoff);
        if seen.insert(delivery_id.to_owned(), std::time::Instant::now()).is_some() {
            tracing::debug!(delivery_id, "github webhook: duplicate delivery, skipping");
            return (StatusCode::OK, Json(json!({ "accepted": false, "duplicate": true })));
        }
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(%error, "github webhook: unparseable payload");
            return (StatusCode::OK, Json(json!({ "accepted": false })));
        }
    };
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();

    let Some((owner, repo)) = github_bot::repository(&payload) else {
        tracing::warn!("github webhook: payload without repository");
        return (StatusCode::OK, Json(json!({ "accepted": false })));
    };

    match github_bot::mention_request(&event, &payload) {
        Some(pr_number) => {
            github_bot::handle_mention(
                state,
                owner.to_owned(),
                repo.to_owned(),
                pr_number,
            );
            (StatusCode::OK, Json(json!({ "accepted": true })))
        }
        None => (StatusCode::OK, Json(json!({ "accepted": false }))),
    }
}

/// Constant-time HMAC-SHA256 check of `X-Hub-Signature-256` against the
/// raw body. Expected format: `sha256=<64 lowercase hex chars>`.
///
/// Test surface only (re-exported via `routes::mod`).
#[doc(hidden)]
pub fn verify_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let Some(hex_digest) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Some(decoded) = hex_decode(hex_digest) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key");
    mac.update(body);
    mac.verify_slice(&decoded).is_ok()
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut it = hex.bytes();
    while let (Some(hi), Some(lo)) = (it.next(), it.next()) {
        bytes.push((nibble(hi)? << 4) | nibble(lo)?);
    }
    Some(bytes)
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}
