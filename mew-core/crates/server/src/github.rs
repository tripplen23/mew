//! GitHub App auth + REST helpers: private key -> signed JWT -> installation
//! access token. The token (~1h lifetime) is what the webhook bot uses for
//! every REST call (fetching diffs, posting reviews). Discovered
//! installation IDs are cached per-account to avoid re-listing on every
//! delivery, and access tokens are cached until near expiry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use chrono::DateTime;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::json;

use crate::config::GithubServerConfig;

const API_ROOT: &str = "https://api.github.com";
const JWT_MAX_SECONDS: i64 = 600;
const INSTALLATIONS_PER_PAGE: u64 = 100;
const MAX_INSTALLATION_PAGES: u64 = 20;

/// GitHub App client: JWT signing + installation token exchange + the REST
/// calls the webhook bot makes. One instance lives in `AppState` for the
/// server's lifetime so the installation/token caches survive deliveries.
#[derive(Clone)]
pub struct GithubClient {
    app_id: u64,
    key: EncodingKey,
    /// Shared reqwest client (webhook routes reuse it for REST calls).
    pub(crate) http: reqwest::Client,
    installation_ids: Arc<Mutex<HashMap<String, u64>>>,
    /// Installation access tokens keyed by owner, cached until near expiry.
    tokens: Arc<Mutex<HashMap<String, (String, i64)>>>,
}

impl GithubClient {
    /// Load the app private key from a PEM file.
    pub fn new(app_id: u64, pem_path: &str) -> Result<Self> {
        let pem = std::fs::read_to_string(pem_path)
            .with_context(|| format!("reading GitHub App private key {pem_path}"))?;
        let key = EncodingKey::from_rsa_pem(pem.as_bytes())
            .context("parsing GitHub App private key (expected RSA PEM)")?;
        Ok(Self {
            app_id,
            key,
            http: reqwest::Client::builder()
                .user_agent("mewcode")
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            installation_ids: Arc::new(Mutex::new(HashMap::new())),
            tokens: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Build from server config; `None` when the app credentials are
    /// incomplete (the webhook route is then disabled).
    pub fn from_config(config: &GithubServerConfig) -> Option<Self> {
        Self::new(config.app_id?, config.private_key_path.as_deref()?).ok()
    }

    /// Installation ID for the account owning `owner`, discovered once and
    /// cached. Errors if the app is not installed there. Follows Link
    /// pagination when the app has more installations than one page.
    pub async fn installation_id(&self, owner: &str) -> Result<u64> {
        if let Some(id) = self.installation_ids.lock().unwrap().get(owner) {
            return Ok(*id);
        }
        let mut url = format!("{API_ROOT}/app/installations?per_page={INSTALLATIONS_PER_PAGE}");
        for _ in 0..MAX_INSTALLATION_PAGES {
            let resp = self
                .http
                .get(&url)
                .bearer_auth(self.jwt()?)
                .send()
                .await
                .context("listing GitHub App installations")?
                .error_for_status()
                .context("GitHub rejected installation listing")?;
            let link = resp
                .headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let page: Vec<serde_json::Value> = resp.json().await?;
            if let Some(id) = page
                .iter()
                .find(|i| i["account"]["login"].as_str() == Some(owner))
                .and_then(|i| i["id"].as_u64())
            {
                self.installation_ids.lock().unwrap().insert(owner.to_owned(), id);
                return Ok(id);
            }
            url = next_page_url(link.as_deref()).ok_or_else(|| {
                anyhow!("GitHub App not installed for {owner} (scanned {MAX_INSTALLATION_PAGES} pages)")
            })?;
        }
        Err(anyhow!("GitHub App not installed for {owner}"))
    }

    /// Installation access token (~1h) for `owner`, cached until 60s before
    /// expiry. A failed exchange evicts the cached installation ID so a
    /// stale ID is not retried forever.
    pub async fn installation_token(&self, owner: &str) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        if let Some((token, expires_at)) = self.tokens.lock().unwrap().get(owner).cloned() {
            if expires_at - now > 60 {
                return Ok(token);
            }
        }
        let id = self.installation_id(owner).await?;
        let resp: serde_json::Value = self
            .http
            .post(format!("{API_ROOT}/app/installations/{id}/access_tokens"))
            .bearer_auth(self.jwt()?)
            .send()
            .await
            .context("exchanging installation access token")?
            .error_for_status()
            .context("GitHub rejected access-token exchange")?
            .json()
            .await?;
        let token = resp["token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("no token in GitHub response"))?;
        let expires_at = resp["expires_at"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(now + 3600);
        self.tokens
            .lock()
            .unwrap()
            .insert(owner.to_owned(), (token.clone(), expires_at));
        Ok(token)
    }

    /// PR diff as raw text (`Accept: application/vnd.github.diff`).
    pub(crate) async fn pr_diff(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<String> {
        let resp = self
            .http
            .get(format!("{API_ROOT}/repos/{owner}/{repo}/pulls/{pr_number}"))
            .bearer_auth(token)
            .header("Accept", "application/vnd.github.diff")
            .send()
            .await
            .context("fetching PR diff")?
            .error_for_status()
            .context("GitHub rejected diff fetch")?;
        Ok(resp.text().await?)
    }

    /// Post review findings as a PR review comment.
    pub(crate) async fn post_review(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        pr_number: u64,
        body: &str,
    ) -> Result<()> {
        self.http
            .post(format!(
                "{API_ROOT}/repos/{owner}/{repo}/pulls/{pr_number}/reviews"
            ))
            .bearer_auth(token)
            .json(&json!({ "body": body, "event": "COMMENT" }))
            .send()
            .await
            .context("posting PR review")?
            .error_for_status()
            .context("GitHub rejected review post")?;
        Ok(())
    }

    /// Post a plain issue/PR comment (used to report review failures).
    pub(crate) async fn post_issue_comment(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        pr_number: u64,
        body: &str,
    ) -> Result<()> {
        self.http
            .post(format!(
                "{API_ROOT}/repos/{owner}/{repo}/issues/{pr_number}/comments"
            ))
            .bearer_auth(token)
            .json(&json!({ "body": body }))
            .send()
            .await
            .context("posting issue comment")?
            .error_for_status()
            .context("GitHub rejected issue comment post")?;
        Ok(())
    }

    /// Post a review with line-anchored inline comments; `body` is the
    /// summary text shown at the top of the review.
    pub(crate) async fn post_review_with_comments(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        pr_number: u64,
        body: &str,
        comments: &[crate::services::github_bot::InlineComment],
    ) -> Result<()> {
        let comments = comments
            .iter()
            .map(|c| json!({ "path": c.path, "line": c.line, "body": c.body }))
            .collect::<Vec<_>>();
        self.http
            .post(format!(
                "{API_ROOT}/repos/{owner}/{repo}/pulls/{pr_number}/reviews"
            ))
            .bearer_auth(token)
            .json(&json!({ "body": body, "event": "COMMENT", "comments": comments }))
            .send()
            .await
            .context("posting PR review with comments")?
            .error_for_status()
            .context("GitHub rejected review post")?;
        Ok(())
    }

    /// App JWT (RS256, ≤10 min, `iss` = app ID). `iat` is backdated 60s so
    /// GitHub's clock-skew tolerance never rejects a fresh token.
    #[doc(hidden)]
    pub fn jwt(&self) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({ "iat": now - 60, "exp": now + JWT_MAX_SECONDS, "iss": self.app_id });
        encode(&Header::new(Algorithm::RS256), &claims, &self.key).context("signing app JWT")
    }
}

/// URL of the next page from a Link header's `rel="next"`, if present.
fn next_page_url(link: Option<&str>) -> Option<String> {
    link?.split(',').find_map(|part| {
        let mut parts = part.split(';');
        let url = parts.next()?.trim().trim_matches('<').trim_matches('>');
        let rel = parts.next()?.trim();
        (rel == "rel=\"next\"").then(|| url.to_owned())
    })
}
