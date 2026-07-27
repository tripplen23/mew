//! Provider credential types — key management separate from model definitions.
//!
//! Credentials are stored in `credentials.yaml` next to the Mew config file.
//! Each provider entry records when the key was last validated so the TUI can
//! show connection status without making a network call every time.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ProviderId;

/// A stored API credential for one provider.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderCredential {
    /// Which provider this credential belongs to.
    pub provider: ProviderId,
    /// API key (opaque string — stored as-is for now, encryption later).
    pub api_key: String,
    /// When the key was last validated via a test API call.
    /// `None` = never validated (uploaded but not tested).
    pub validated_at: Option<String>,
    /// Human-readable label (e.g. "Work account").
    #[serde(default)]
    pub label: Option<String>,
}

impl ProviderCredential {
    /// Create a new unvalidated credential.
    pub fn new(provider: ProviderId, api_key: String) -> Self {
        Self {
            provider,
            api_key,
            validated_at: None,
            label: None,
        }
    }
}

/// Request body for `POST /providers/connect`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectProviderRequest {
    /// Which provider to connect.
    pub provider: ProviderId,
    /// The API key to validate and store.
    pub api_key: String,
}

/// Response for `POST /providers/connect`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status")]
pub enum ConnectProviderResponse {
    /// Key validated successfully.
    #[serde(rename = "ok")]
    Ok {
        provider: ProviderId,
        /// ISO-8601 timestamp of validation.
        validated_at: String,
    },
    /// Key was rejected by the provider.
    #[serde(rename = "invalid-key")]
    InvalidKey {
        provider: ProviderId,
        /// Provider's error message (e.g. "401 Unauthorized").
        reason: String,
    },
    /// Network or timeout error during validation.
    #[serde(rename = "error")]
    Error {
        provider: ProviderId,
        message: String,
    },
}

/// Summary sent to the TUI for each provider's connection status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderStatus {
    pub provider: ProviderId,
    /// Whether a credential exists for this provider.
    pub connected: bool,
    /// When the key was last validated (if connected).
    pub validated_at: Option<String>,
    /// Human-friendly label (if set).
    pub label: Option<String>,
}

impl fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.connected {
            write!(f, "{} ✓", self.provider)
        } else {
            write!(f, "{} (not connected)", self.provider)
        }
    }
}
