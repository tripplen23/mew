use std::str::FromStr;

/// All models reachable through an OpenCode Go subscription. Some hit
/// the Anthropic-compatible `/v1/messages` endpoint, the rest hit
/// OpenAI-compatible `/v1/chat/completions`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
pub enum ModelId {
    // === Anthropic-compatible /v1/messages ===
    #[serde(rename = "minimax-m3")]
    MiniMaxM3,

    #[serde(rename = "minimax-m2.7")]
    MiniMaxM27,

    #[serde(rename = "minimax-m2.5")]
    MiniMaxM25,

    #[serde(rename = "qwen3.7-max")]
    Qwen37Max,

    #[serde(rename = "qwen3.7-plus")]
    Qwen37Plus,

    #[serde(rename = "qwen3.6-plus")]
    Qwen36Plus,

    // === OpenAI-compatible /v1/chat/completions ===
    #[serde(rename = "glm-5.2")]
    Glm52,

    #[serde(rename = "glm-5.1")]
    Glm51,

    #[serde(rename = "glm-5")]
    Glm5,

    #[serde(rename = "kimi-k2.7-code")]
    KimiK27Code,

    #[serde(rename = "kimi-k2.6")]
    KimiK26,

    #[serde(rename = "mimo-v2.5")]
    MiMoV25,

    #[serde(rename = "mimo-v2.5-pro")]
    MiMoV25Pro,

    #[serde(rename = "deepseek-v4-pro")]
    DeepSeekV4Pro,

    #[serde(rename = "deepseek-v4-flash")]
    DeepSeekV4Flash,
}

impl ModelId {
    /// Wire id of the default model. Used in `as_str()` and in tests.
    pub const MINIMAX_M3_ID: &'static str = "minimax-m3";

    /// All supported models in display order.
    pub const ALL: &'static [ModelId] = &[
        ModelId::MiniMaxM3,
        ModelId::MiniMaxM27,
        ModelId::MiniMaxM25,
        ModelId::Qwen37Max,
        ModelId::Qwen37Plus,
        ModelId::Qwen36Plus,
        ModelId::Glm52,
        ModelId::Glm51,
        ModelId::Glm5,
        ModelId::KimiK27Code,
        ModelId::KimiK26,
        ModelId::MiMoV25,
        ModelId::MiMoV25Pro,
        ModelId::DeepSeekV4Pro,
        ModelId::DeepSeekV4Flash,
    ];

    /// Which OpenCode Go endpoint serves this model.
    pub fn kind(self) -> ModelKind {
        match self {
            ModelId::MiniMaxM3
            | ModelId::MiniMaxM27
            | ModelId::MiniMaxM25
            | ModelId::Qwen37Max
            | ModelId::Qwen37Plus
            | ModelId::Qwen36Plus => ModelKind::AnthropicMessages,
            ModelId::Glm52
            | ModelId::Glm51
            | ModelId::Glm5
            | ModelId::KimiK27Code
            | ModelId::KimiK26
            | ModelId::MiMoV25
            | ModelId::MiMoV25Pro
            | ModelId::DeepSeekV4Pro
            | ModelId::DeepSeekV4Flash => ModelKind::OpenAiChatCompletions,
        }
    }

    /// Wire id of the model sent to the OpenCode Go API.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelId::MiniMaxM3 => Self::MINIMAX_M3_ID,
            ModelId::MiniMaxM27 => "minimax-m2.7",
            ModelId::MiniMaxM25 => "minimax-m2.5",
            ModelId::Qwen37Max => "qwen3.7-max",
            ModelId::Qwen37Plus => "qwen3.7-plus",
            ModelId::Qwen36Plus => "qwen3.6-plus",
            ModelId::Glm52 => "glm-5.2",
            ModelId::Glm51 => "glm-5.1",
            ModelId::Glm5 => "glm-5",
            ModelId::KimiK27Code => "kimi-k2.7-code",
            ModelId::KimiK26 => "kimi-k2.6",
            ModelId::MiMoV25 => "mimo-v2.5",
            ModelId::MiMoV25Pro => "mimo-v2.5-pro",
            ModelId::DeepSeekV4Pro => "deepseek-v4-pro",
            ModelId::DeepSeekV4Flash => "deepseek-v4-flash",
        }
    }

    /// Human-friendly display name for the model picker.
    pub fn display_name(self) -> &'static str {
        match self {
            ModelId::MiniMaxM3 => "MiniMax M3",
            ModelId::MiniMaxM27 => "MiniMax M2.7",
            ModelId::MiniMaxM25 => "MiniMax M2.5",
            ModelId::Qwen37Max => "Qwen 3.7 Max",
            ModelId::Qwen37Plus => "Qwen 3.7 Plus",
            ModelId::Qwen36Plus => "Qwen 3.6 Plus",
            ModelId::Glm52 => "GLM 5.2",
            ModelId::Glm51 => "GLM 5.1",
            ModelId::Glm5 => "GLM 5",
            ModelId::KimiK27Code => "Kimi K2.7 Code",
            ModelId::KimiK26 => "Kimi K2.6",
            ModelId::MiMoV25 => "MiMo V2.5",
            ModelId::MiMoV25Pro => "MiMo V2.5 Pro",
            ModelId::DeepSeekV4Pro => "DeepSeek V4 Pro",
            ModelId::DeepSeekV4Flash => "DeepSeek V4 Flash",
        }
    }

    /// Default model used when none is specified.
    pub const DEFAULT: ModelId = ModelId::MiniMaxM3;
}

impl Default for ModelId {
    fn default() -> Self {
        ModelId::DEFAULT
    }
}

impl FromStr for ModelId {
    type Err = ModelIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|m| m.as_str() == s || m.display_name().eq_ignore_ascii_case(s))
            .ok_or_else(|| ModelIdParseError(s.to_string()))
    }
}

/// Which OpenCode Go endpoint a model is served from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ModelKind {
    /// `/v1/messages` (Anthropic-compatible).
    AnthropicMessages,
    /// `/v1/chat/completions` (OpenAI-compatible).
    OpenAiChatCompletions,
}

/// Error returned when a string cannot be parsed into a [`ModelId`].
#[derive(Debug, thiserror::Error)]
#[error("unsupported model: {0}")]
pub struct ModelIdParseError(pub String);
