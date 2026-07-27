//! Credential store — reads and writes API keys to a YAML file.
//!
//! Keys are stored in `~/.config/mew/credentials.yaml`. The engine
//! loads from here first, then falls back to environment variables.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use mewcode_protocol::ProviderId;
use mewcode_protocol::credential::{
    ConnectProviderRequest, ConnectProviderResponse, ProviderCredential, ProviderStatus,
};

use crate::error::EngineError;

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
    credentials: HashMap<ProviderId, ProviderCredential>,
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
                label: self
                    .credentials
                    .get(&provider)
                    .and_then(|c| c.label.clone()),
            })
            .collect()
    }

    /// Validate and store a new credential.
    /// Makes a test API call to the provider to verify the key works.
    pub async fn connect(&mut self, req: ConnectProviderRequest) -> ConnectProviderResponse {
        let ConnectProviderRequest { provider, api_key } = req;

        // Validate the key with a test call.
        match validate_key(provider, &api_key).await {
            Ok(validated_at) => {
                let credential = ProviderCredential {
                    provider,
                    api_key: api_key.clone(),
                    validated_at: Some(validated_at),
                    label: None,
                };
                self.credentials.insert(provider, credential);
                if let Err(e) = self.save() {
                    return ConnectProviderResponse::Error {
                        provider,
                        message: format!("key validated but failed to save: {e}"),
                    };
                }
                ConnectProviderResponse::Ok {
                    provider,
                    validated_at: chrono::Utc::now().to_rfc3339(),
                }
            }
            Err(reason) => ConnectProviderResponse::InvalidKey { provider, reason },
        }
    }

    /// Persist credentials to disk.
    fn save(&self) -> Result<(), EngineError> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| EngineError::Other(format!("failed to create config dir: {e}")))?;
        let list: Vec<&ProviderCredential> = self.credentials.values().collect();
        let yaml = serde_yaml::to_string(&list)
            .map_err(|e| EngineError::Other(format!("failed to serialize credentials: {e}")))?;
        std::fs::write(credentials_path(), yaml)
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
    }
}

/// Make a test API call to verify the key works.
/// Returns the ISO-8601 timestamp on success, or an error message.
async fn validate_key(provider: ProviderId, api_key: &str) -> Result<String, String> {
    let (url, auth_header) = match provider {
        ProviderId::OpenCodeGo => (
            "https://opencode.ai/zen/go/v1/models",
            format!("Bearer {api_key}"),
        ),
        ProviderId::OpenAi => (
            "https://api.openai.com/v1/models",
            format!("Bearer {api_key}"),
        ),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;

    let resp = client
        .get(url)
        .header("Authorization", &auth_header)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "connection timed out — check your network".to_string()
            } else if e.is_connect() {
                format!("could not reach {provider} — check your network")
            } else {
                format!("network error: {e}")
            }
        })?;

    match resp.status() {
        reqwest::StatusCode::OK => Ok(chrono::Utc::now().to_rfc3339()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "invalid API key — {provider} returned {status}: {body}",
            ))
        }
        status => Err(format!(
            "unexpected response from {provider}: HTTP {status}"
        )),
    }
}
