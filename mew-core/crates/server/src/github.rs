//! GitHub App auth + REST helpers: private key -> signed JWT -> installation
//! access token. The token (~1h lifetime) is what the webhook bot uses for
//! every REST call (fetching diffs, posting reviews). Discovered
//! installation IDs are cached per-account to avoid re-listing on every
//! delivery.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::json;

use crate::AppState;

const API_ROOT: &str = "https://api.github.com";
const JWT_MAX_MINUTES: u64 = 10;

/// GitHub App client: JWT signing + installation token exchange + the REST
/// calls the webhook bot makes.
pub struct GithubClient {
    app_id: u64,
    key: EncodingKey,
    /// Shared reqwest client (webhook routes reuse it for REST calls).
    pub(crate) http: reqwest::Client,
    installation_ids: Mutex<HashMap<String, u64>>,
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
            installation_ids: Mutex::new(HashMap::new()),
        })
    }

    /// Build from server config; errors when the app credentials are
    /// missing.
    pub fn from_state(state: &AppState) -> Result<Self> {
        let app_id = state
            .config
            .github_app_id
            .ok_or_else(|| anyhow!("github app id not configured"))?;
        let key_path = state
            .config
            .github_private_key_path
            .as_deref()
            .ok_or_else(|| anyhow!("github private key path not configured"))?;
        Self::new(app_id, key_path)
    }

    /// Installation ID for the account owning `owner`, discovered once and
    /// cached. Errors if the app is not installed there.
    pub async fn installation_id(&self, owner: &str) -> Result<u64> {
        if let Some(id) = self.installation_ids.lock().unwrap().get(owner) {
            return Ok(*id);
        }
        let resp: Vec<serde_json::Value> = self
            .http
            .get(format!("{API_ROOT}/app/installations"))
            .bearer_auth(self.jwt()?)
            .send()
            .await
            .context("listing GitHub App installations")?
            .error_for_status()
            .context("GitHub rejected installation listing")?
            .json()
            .await?;
        let id = resp
            .iter()
            .find(|i| i["account"]["login"].as_str() == Some(owner))
            .and_then(|i| i["id"].as_u64())
            .ok_or_else(|| anyhow!("GitHub App not installed for {owner}"))?;
        self.installation_ids.lock().unwrap().insert(owner.to_owned(), id);
        Ok(id)
    }

    /// Installation access token (~1h) for `owner`.
    pub async fn installation_token(&self, owner: &str) -> Result<String> {
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
        resp["token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("no token in GitHub response"))
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

    /// App JWT (RS256, ≤10 min, `iss` = app ID).
    fn jwt(&self) -> Result<String> {
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = json!({ "iat": now, "exp": now + JWT_MAX_MINUTES * 60, "iss": self.app_id });
        encode(&Header::new(Algorithm::RS256), &claims, &self.key).context("signing app JWT")
    }
}
