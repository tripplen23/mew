use std::fmt;
use std::str::FromStr;

/// Which provider serves a model. Used for credential resolution,
/// base URL selection, and client-side grouping.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    /// OpenCode Go subscription (default).
    OpenCodeGo,
    /// Native OpenAI API via api.openai.com.
    OpenAi,
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderId::OpenCodeGo => write!(f, "OpenCode Go"),
            ProviderId::OpenAi => write!(f, "OpenAI"),
        }
    }
}

/// Which endpoint protocol a model speaks.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ModelKind {
    /// `/v1/messages` (Anthropic-compatible).
    AnthropicMessages,
    /// `/v1/chat/completions` via OpenCode Go relay.
    OpenCodeGo,
    /// `/v1/chat/completions` via native OpenAI API.
    OpenAi,
}

macro_rules! define_models {
    ($($variant:ident, $id:literal, $display:literal, $provider:ident, $kind:ident;)+) => {
        /// All models reachable through an OpenCode Go subscription or
        /// a configured native provider.
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
        )]
        pub enum ModelId {
            $(#[serde(rename = $id)] $variant,)+
        }

        impl ModelId {
            /// Wire id of the default model. Used in `as_str()` and in tests.
            pub const MINIMAX_M3_ID: &'static str = "minimax-m3";

            /// All supported models in display order.
            pub const ALL: &'static [ModelId] = &[$(ModelId::$variant,)+];

            /// Which provider serves this model.
            pub fn provider(self) -> ProviderId {
                match self { $(ModelId::$variant => ProviderId::$provider,)+ }
            }

            /// Which endpoint protocol this model speaks.
            pub fn kind(self) -> ModelKind {
                match self { $(ModelId::$variant => ModelKind::$kind,)+ }
            }

            /// Wire id of the model sent to the API.
            pub fn as_str(self) -> &'static str {
                match self { $(ModelId::$variant => $id,)+ }
            }

            /// Human-friendly display name for the model picker.
            pub fn display_name(self) -> &'static str {
                match self { $(ModelId::$variant => $display,)+ }
            }

            /// Default model used when none is specified.
            pub const DEFAULT: ModelId = ModelId::MiniMaxM3;
        }
    };
}

define_models! {
    MiniMaxM3, "minimax-m3", "MiniMax M3", OpenCodeGo, AnthropicMessages;
    MiniMaxM27, "minimax-m2.7", "MiniMax M2.7", OpenCodeGo, AnthropicMessages;
    MiniMaxM25, "minimax-m2.5", "MiniMax M2.5", OpenCodeGo, AnthropicMessages;
    Qwen37Max, "qwen3.7-max", "Qwen 3.7 Max", OpenCodeGo, AnthropicMessages;
    Qwen37Plus, "qwen3.7-plus", "Qwen 3.7 Plus", OpenCodeGo, AnthropicMessages;
    Qwen36Plus, "qwen3.6-plus", "Qwen 3.6 Plus", OpenCodeGo, AnthropicMessages;
    Glm52, "glm-5.2", "GLM 5.2", OpenCodeGo, OpenCodeGo;
    Glm51, "glm-5.1", "GLM 5.1", OpenCodeGo, OpenCodeGo;
    Glm5, "glm-5", "GLM 5", OpenCodeGo, OpenCodeGo;
    KimiK27Code, "kimi-k2.7-code", "Kimi K2.7 Code", OpenCodeGo, OpenCodeGo;
    KimiK26, "kimi-k2.6", "Kimi K2.6", OpenCodeGo, OpenCodeGo;
    MiMoV25, "mimo-v2.5", "MiMo V2.5", OpenCodeGo, OpenCodeGo;
    MiMoV25Pro, "mimo-v2.5-pro", "MiMo V2.5 Pro", OpenCodeGo, OpenCodeGo;
    DeepSeekV4Pro, "deepseek-v4-pro", "DeepSeek V4 Pro", OpenCodeGo, OpenCodeGo;
    DeepSeekV4Flash, "deepseek-v4-flash", "DeepSeek V4 Flash", OpenCodeGo, OpenCodeGo;
    Gpt41, "gpt-4.1", "GPT-4.1", OpenAi, OpenAi;
    Gpt41Mini, "gpt-4.1-mini", "GPT-4.1 Mini", OpenAi, OpenAi;
    Gpt41Nano, "gpt-4.1-nano", "GPT-4.1 Nano", OpenAi, OpenAi;
    Gpt4o, "gpt-4o", "GPT-4o", OpenAi, OpenAi;
    Gpt4oMini, "gpt-4o-mini", "GPT-4o Mini", OpenAi, OpenAi;
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

/// Error returned when a string cannot be parsed into a [`ModelId`].
#[derive(Debug, thiserror::Error)]
#[error("unsupported model: {0}")]
pub struct ModelIdParseError(pub String);
