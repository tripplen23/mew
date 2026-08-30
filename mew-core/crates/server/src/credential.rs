//! Credential store — reads and writes API keys to a YAML file.
//!
//! Keys are stored in `~/.config/mew/credentials.yaml`. The engine
//! loads from here first, then falls back to environment variables.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use mewcode_engine::EngineError;
use mewcode_protocol::credential::{ProviderCredential, ProviderStatus};
use mewcode_protocol::env::{
    ANTHROPIC_API_KEY, DEEPSEEK_API_KEY, OPENAI_API_KEY, OPENCODE_GO_API_KEY, OPENCODE_ZEN_API_KEY,
    OPENROUTER_API_KEY,
};
use mewcode_protocol::{ProviderId, SUPPORTED_PROVIDERS};

/// File name inside the Mew config directory.
const CREDENTIALS_FILE: &str = "credentials.yaml";

/// Where Mew stores its configuration.
fn config_dir() -> Result<PathBuf, EngineError> {
    dirs::config_dir()
        .map(|dir| dir.join("mew"))
        .ok_or_else(|| EngineError::Other("could not resolve config directory".into()))
}

fn credentials_path() -> Result<PathBuf, EngineError> {
    Ok(config_dir()?.join(CREDENTIALS_FILE))
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
        Self::load_at(&credentials_path()?)
    }

    #[doc(hidden)]
    pub fn load_at(path: &std::path::Path) -> Result<Self, EngineError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut file = std::fs::File::open(path)
            .map_err(|e| EngineError::Other(format!("failed to read credentials file: {e}")))?;
        if !file
            .metadata()
            .map_err(|e| EngineError::Other(format!("failed to inspect credentials file: {e}")))?
            .is_file()
        {
            return Err(EngineError::Other(
                "credentials path is not a regular file".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| EngineError::Other(format!("failed to secure config dir: {e}")))?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| {
                    EngineError::Other(format!("failed to secure credentials file: {e}"))
                })?;
        }
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut file, &mut contents)
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
        SUPPORTED_PROVIDERS
            .iter()
            .map(|descriptor| descriptor.id)
            .map(|provider| ProviderStatus {
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
        self.store_at(credential, &credentials_path()?)
    }

    #[doc(hidden)]
    pub fn store_at(
        &mut self,
        credential: ProviderCredential,
        path: &std::path::Path,
    ) -> Result<(), EngineError> {
        let mut candidate = self.clone();
        candidate
            .credentials
            .insert(credential.provider, credential);
        candidate.save_at(path)?;
        *self = candidate;
        Ok(())
    }

    /// Persist credentials to disk with atomic write and restricted permissions.
    pub fn save(&self) -> Result<(), EngineError> {
        self.save_at(&credentials_path()?)
    }

    fn save_at(&self, path: &std::path::Path) -> Result<(), EngineError> {
        let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(dir)
            .map_err(|e| EngineError::Other(format!("failed to create config dir: {e}")))?;
        #[cfg(unix)]
        std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .map_err(|e| EngineError::Other(format!("failed to secure config dir: {e}")))?;
        let list: Vec<&ProviderCredential> = self.credentials.values().collect();
        let yaml = serde_yaml::to_string(&list)
            .map_err(|e| EngineError::Other(format!("failed to serialize credentials: {e}")))?;

        let tmp_path = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), EngineError> {
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
            file.sync_all()
                .map_err(|e| EngineError::Other(format!("failed to sync credentials: {e}")))?;
            std::fs::rename(&tmp_path, path).map_err(|e| {
                EngineError::Other(format!("failed to write credentials file: {e}"))
            })?;
            if let Err(error) = std::fs::File::open(dir).and_then(|directory| directory.sync_all())
            {
                tracing::warn!(%error, "credentials committed but config directory sync failed");
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        result
    }
}

/// Try to get an API key from the environment.
fn env_key(provider: ProviderId) -> Option<String> {
    match provider {
        ProviderId::OpenCodeGo => env::var(OPENCODE_GO_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::OpenCodeZen => env::var(OPENCODE_ZEN_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::OpenAi => env::var(OPENAI_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::Anthropic => env::var(ANTHROPIC_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::DeepSeek => env::var(DEEPSEEK_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty()),
        ProviderId::OpenRouter => env::var(OPENROUTER_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty()),
    }
}

pub const OPENROUTER_KEY_URL: &str = "https://openrouter.ai/api/v1/key";

/// Credential validation outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    InvalidKey(String),
    Unavailable(String),
}

/// Make a test API call to verify the key works.
/// Returns the ISO-8601 timestamp on success, preserving invalid-key versus upstream failures.
pub async fn validate_key(provider: ProviderId, api_key: &str) -> Result<String, ValidationError> {
    if invalid_key_header(provider, api_key) {
        return Err(ValidationError::InvalidKey("invalid API key".into()));
    }
    if provider == ProviderId::OpenRouter {
        return validate_openrouter_key(api_key).await;
    }
    let result = match provider {
        ProviderId::OpenCodeGo => validate_opencodego_key(api_key).await,
        ProviderId::OpenCodeZen => {
            validate_bearer_models_key(api_key, "https://opencode.ai/zen/v1/models", "OpenCode Zen")
                .await
        }
        ProviderId::OpenAi => validate_openai_key(api_key).await,
        ProviderId::Anthropic => validate_provider_key_at(
            provider,
            api_key,
            "https://api.anthropic.com/v1/models",
            std::time::Duration::from_secs(15),
        )
        .await
        .map_err(|error| match error {
            ValidationError::InvalidKey(message) | ValidationError::Unavailable(message) => message,
        }),
        ProviderId::DeepSeek => validate_deepseek_key(api_key).await,
        ProviderId::OpenRouter => unreachable!("handled above"),
    };
    result.map_err(|message| {
        if message.starts_with("invalid API key") {
            ValidationError::InvalidKey(message)
        } else {
            ValidationError::Unavailable(message)
        }
    })
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
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(format!(
            "invalid API key — OpenAI returned {}",
            resp.status()
        )),
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
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(format!(
            "invalid API key — OpenCode Go returned {}",
            resp.status()
        )),
        status => Err(format!(
            "validation failed — OpenCode Go returned HTTP {status}"
        )),
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
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(format!(
            "invalid API key — DeepSeek returned {}",
            resp.status()
        )),
        status => Err(format!("unexpected response from DeepSeek: HTTP {status}")),
    }
}

async fn validate_bearer_models_key(
    api_key: &str,
    url: &str,
    provider: &str,
) -> Result<String, String> {
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| format!("failed to create HTTP client: {error}"))?
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| network_error(error, provider))?;
    match response.status() {
        reqwest::StatusCode::OK => Ok(chrono::Utc::now().to_rfc3339()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(format!(
            "invalid API key — {provider} returned {}",
            response.status()
        )),
        status => Err(format!(
            "unexpected response from {provider}: HTTP {status}"
        )),
    }
}

/// Validate a provider key against an override endpoint used by local tests.
#[doc(hidden)]
pub async fn validate_provider_key_at(
    provider: ProviderId,
    api_key: &str,
    url: &str,
    timeout: std::time::Duration,
) -> Result<String, ValidationError> {
    if invalid_key_header(provider, api_key) {
        return Err(ValidationError::InvalidKey("invalid API key".into()));
    }
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            ValidationError::Unavailable(format!("failed to create HTTP client: {error}"))
        })?;
    let request = client.get(url);
    let request = if provider == ProviderId::Anthropic {
        request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
    } else {
        request.bearer_auth(api_key)
    };
    let response = request.send().await.map_err(|error| {
        ValidationError::Unavailable(network_error(error, &provider.to_string()))
    })?;
    match response.status() {
        reqwest::StatusCode::OK => Ok(chrono::Utc::now().to_rfc3339()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(
            ValidationError::InvalidKey(format!("invalid API key — {provider} rejected it")),
        ),
        status => Err(ValidationError::Unavailable(format!(
            "unexpected response from {provider}: HTTP {status}"
        ))),
    }
}

fn invalid_key_header(provider: ProviderId, api_key: &str) -> bool {
    api_key.trim().is_empty()
        || if provider == ProviderId::Anthropic {
            reqwest::header::HeaderValue::from_str(api_key).is_err()
        } else {
            reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}")).is_err()
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

#[doc(hidden)]
pub fn classify_openrouter_status(status: reqwest::StatusCode) -> Option<ValidationError> {
    match status {
        reqwest::StatusCode::OK => None,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Some(
            ValidationError::InvalidKey(format!("invalid API key — OpenRouter returned {status}")),
        ),
        status => Some(ValidationError::Unavailable(format!(
            "unexpected response from OpenRouter: HTTP {status}"
        ))),
    }
}

async fn validate_openrouter_key(api_key: &str) -> Result<String, ValidationError> {
    validate_openrouter_key_at(
        api_key,
        OPENROUTER_KEY_URL,
        std::time::Duration::from_secs(15),
    )
    .await
}

#[doc(hidden)]
pub async fn validate_openrouter_key_at(
    api_key: &str,
    url: &str,
    timeout: std::time::Duration,
) -> Result<String, ValidationError> {
    if invalid_key_header(ProviderId::OpenRouter, api_key) {
        return Err(ValidationError::InvalidKey("invalid API key".into()));
    }
    let resp = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| {
            ValidationError::Unavailable(format!("failed to create HTTP client: {error}"))
        })?
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| ValidationError::Unavailable(network_error(error, "OpenRouter")))?;

    match classify_openrouter_status(resp.status()) {
        None => Ok(chrono::Utc::now().to_rfc3339()),
        Some(error) => Err(error),
    }
}
