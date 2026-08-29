//! Credential store — reads and writes API keys to a YAML file.
//!
//! Keys are stored in `~/.config/mew/credentials.yaml`. The engine
//! loads from here first, then falls back to environment variables.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use mewcode_engine::EngineError;
use mewcode_protocol::ProviderId;
use mewcode_protocol::credential::{ProviderCredential, ProviderStatus};

/// File name inside the Mew config directory.
const CREDENTIALS_FILE: &str = "credentials.yaml";

/// Where Mew stores its configuration.
fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mew")
}

fn credentials_path() -> PathBuf {
    config_dir().join(CREDENTIALS_FILE)
}

/// In-memory view of stored credentials.
#[derive(Debug, Clone, Default)]
pub struct CredentialStore {
    /// Stored per-provider credentials. Public so the app can inspect or (in
    /// tests) control the store; the resolution chain is `build_engine_config`.
    pub credentials: HashMap<ProviderId, ProviderCredential>,
}

impl CredentialStore {
    /// Load credentials from disk. Returns empty store if file doesn't exist.
    pub fn load() -> Result<Self, EngineError> {
        let path = credentials_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| EngineError::Other(format!("failed to read credentials file: {e}")))?;
        let list: Vec<ProviderCredential> = serde_yaml::from_str(&contents)
            .map_err(|e| EngineError::Other(format!("invalid credentials file: {e}")))?;
        let credentials = list.into_iter().map(|c| (c.provider, c)).collect();
        Ok(Self { credentials })
    }

    /// Get the API key for a provider.
    /// Falls back to environment variables if no stored credential exists.
    pub fn api_key(&self, provider: ProviderId) -> Option<String> {
        self.credentials
            .get(&provider)
            .map(|c| c.api_key.clone())
            .or_else(|| env_key(provider))
    }

    /// Whether a credential exists (stored or env) for this provider.
    pub fn has(&self, provider: ProviderId) -> bool {
        self.credentials.contains_key(&provider) || env_key(provider).is_some()
    }

    /// Connection status for all known providers.
    pub fn status(&self) -> Vec<ProviderStatus> {
        [ProviderId::OpenCodeGo, ProviderId::OpenAi]
            .iter()
            .map(|&provider| ProviderStatus {
                provider,
                connected: self.has(provider),
                validated_at: self
                    .credentials
                    .get(&provider)
                    .and_then(|c| c.validated_at.clone()),
            })
            .collect()
    }

    /// Store a validated credential and persist atomically to disk.
    pub fn store(&mut self, credential: ProviderCredential) -> Result<(), EngineError> {
        self.credentials.insert(credential.provider, credential);
        self.save()
    }

    /// Persist credentials to disk with atomic write and restricted permissions.
    pub fn save(&self) -> Result<(), EngineError> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| EngineError::Other(format!("failed to create config dir: {e}")))?;
        let list: Vec<&ProviderCredential> = self.credentials.values().collect();
        let yaml = serde_yaml::to_string(&list)
            .map_err(|e| EngineError::Other(format!("failed to serialize credentials: {e}")))?;

        // Atomic write: create temp file with restricted permissions, then rename.
        let tmp_path = credentials_path().with_extension("yaml.tmp");
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut file = opts.open(&tmp_path).map_err(|e| {
                EngineError::Other(format!("failed to create temp credentials file: {e}"))
            })?;
            file.write_all(yaml.as_bytes())
                .map_err(|e| EngineError::Other(format!("failed to write credentials: {e}")))?;
        }
        std::fs::rename(&tmp_path, credentials_path())
            .map_err(|e| EngineError::Other(format!("failed to write credentials file: {e}")))?;
        Ok(())
    }
}

/// Try to get an API key from the environment.
fn env_key(provider: ProviderId) -> Option<String> {
    match provider {
        ProviderId::OpenCodeGo => env::var("OPENCODE_GO_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::OpenAi => env::var("OPENAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::DeepSeek => env::var("DEEPSEEK_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    }
}

/// Make a test API call to verify the key works.
/// Returns the ISO-8601 timestamp on success, or an error message.
pub async fn validate_key(provider: ProviderId, api_key: &str) -> Result<String, String> {
    match provider {
        ProviderId::OpenCodeGo => validate_opencodego_key(api_key).await,
        ProviderId::OpenAi => validate_openai_key(api_key).await,
        // DeepSeek uses the same OpenAI-compatible API format for validation.
        ProviderId::DeepSeek => validate_deepseek_key(api_key).await,
    }
}

/// Validate an OpenAI key by listing models (requires auth).
async fn validate_openai_key(api_key: &str) -> Result<String, String> {
    let url = "https://api.openai.com/v1/models";
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| network_error(e, "OpenAI"))?;

    match resp.status() {
        reqwest::StatusCode::OK => Ok(chrono::Utc::now().to_rfc3339()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "invalid API key — OpenAI returned {status}: {body}",
            ))
        }
        status => Err(format!("unexpected response from OpenAI: HTTP {status}")),
    }
}

/// Validate an OpenCode Go key by making a minimal chat completion.
/// The `/models` endpoint is public (always returns 200), so we use a
/// real inference call with max_tokens=1 to verify the key.
async fn validate_opencodego_key(api_key: &str) -> Result<String, String> {
    let url = "https://opencode.ai/zen/go/v1/chat/completions";
    let body = serde_json::json!({
        "model": "minimax-m3",
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false
    });

    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| network_error(e, "OpenCode Go"))?;

    match resp.status() {
        reqwest::StatusCode::OK => Ok(chrono::Utc::now().to_rfc3339()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "invalid API key — OpenCode Go returned {status}: {body}",
            ))
        }
        status => {
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "validation failed — OpenCode Go returned HTTP {status}: {body}",
            ))
        }
    }
}

// Validate a DeepSeek key by listing models (requires auth, same as OpenAI format).
async fn validate_deepseek_key(api_key: &str) -> Result<String, String> {
    let url = "https://api.deepseek.com/models";
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| network_error(e, "DeepSeek"))?;

    match resp.status() {
        reqwest::StatusCode::OK => Ok(chrono::Utc::now().to_rfc3339()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "invalid API key — DeepSeek returned {status}: {body}",
            ))
        }
        status => Err(format!("unexpected response from DeepSeek: HTTP {status}")),
    }
}

fn network_error(e: reqwest::Error, provider: &str) -> String {
    if e.is_timeout() {
        "connection timed out — check your network".to_string()
    } else if e.is_connect() {
        format!("could not reach {provider} — check your network")
    } else {
        format!("network error: {e}")
    }
}
