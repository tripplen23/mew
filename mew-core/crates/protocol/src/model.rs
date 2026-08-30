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
    /// OpenCode Zen multi-transport API.
    OpenCodeZen,
    /// Native OpenAI API via api.openai.com.
    OpenAi,
    /// Native Anthropic API via api.anthropic.com.
    Anthropic,
    /// Native DeepSeek API via api.deepseek.com.
    DeepSeek,
    /// OpenRouter's OpenAI-compatible API.
    OpenRouter,
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderId::OpenCodeGo => write!(f, "OpenCode Go"),
            ProviderId::OpenCodeZen => write!(f, "OpenCode Zen"),
            ProviderId::OpenAi => write!(f, "OpenAI"),
            ProviderId::Anthropic => write!(f, "Anthropic"),
            ProviderId::DeepSeek => write!(f, "DeepSeek"),
            ProviderId::OpenRouter => write!(f, "OpenRouter"),
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
    /// `/v1/chat/completions` via OpenCode Zen.
    OpenCodeZen,
    /// `/v1/chat/completions` via native OpenAI API.
    OpenAi,
    /// `/v1/responses` (OpenAI-compatible).
    OpenAiResponses,
    /// `/v1/chat/completions` via native DeepSeek API.
    DeepSeek,
    /// `/v1/chat/completions` via OpenRouter.
    OpenRouter,
}

macro_rules! define_models {
    ($($variant:ident, $id:literal, $display:literal, $provider:ident, $kind:ident, $ctx_limit:expr;)+) => {
        /// Models with legacy unqualified session identities. Runtime provider
        /// catalogs are discovered over HTTP and are not sourced from this list.
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
        )]
        pub enum ModelId {
            $(#[serde(rename = $id)] $variant,)+
        }

        impl ModelId {
            /// Wire id of the MiniMax M3 model. Used in `as_str()` and in tests.
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

            /// Known input token capacity. Returns 0 when the limit is
            /// unknown or unlimited, which disables compaction for that model.
            pub fn context_limit(self) -> u64 {
                match self { $(ModelId::$variant => $ctx_limit,)+ }
            }

            /// Default model used when none is specified.
            pub const DEFAULT: ModelId = ModelId::DeepSeekV4Flash;
        }
    };
}

define_models! {
    MiniMaxM3, "minimax-m3", "MiniMax M3", OpenCodeGo, AnthropicMessages, 200_000;
    MiniMaxM27, "minimax-m2.7", "MiniMax M2.7", OpenCodeGo, AnthropicMessages, 200_000;
    MiniMaxM25, "minimax-m2.5", "MiniMax M2.5", OpenCodeGo, AnthropicMessages, 200_000;
    Qwen37Max, "qwen3.7-max", "Qwen 3.7 Max", OpenCodeGo, AnthropicMessages, 131_072;
    Qwen37Plus, "qwen3.7-plus", "Qwen 3.7 Plus", OpenCodeGo, AnthropicMessages, 131_072;
    Qwen36Plus, "qwen3.6-plus", "Qwen 3.6 Plus", OpenCodeGo, AnthropicMessages, 131_072;
    Glm52, "glm-5.2", "GLM 5.2", OpenCodeGo, OpenCodeGo, 131_072;
    Glm51, "glm-5.1", "GLM 5.1", OpenCodeGo, OpenCodeGo, 131_072;
    Glm5, "glm-5", "GLM 5", OpenCodeGo, OpenCodeGo, 131_072;
    KimiK27Code, "kimi-k2.7-code", "Kimi K2.7 Code", OpenCodeGo, OpenCodeGo, 131_072;
    KimiK26, "kimi-k2.6", "Kimi K2.6", OpenCodeGo, OpenCodeGo, 131_072;
    MiMoV25, "mimo-v2.5", "MiMo V2.5", OpenCodeGo, OpenCodeGo, 131_072;
    MiMoV25Pro, "mimo-v2.5-pro", "MiMo V2.5 Pro", OpenCodeGo, OpenCodeGo, 131_072;
    DeepSeekV4Pro, "deepseek-v4-pro", "DeepSeek V4 Pro", OpenCodeGo, OpenCodeGo, 1_000_000;
    DeepSeekV4Flash, "deepseek-v4-flash", "DeepSeek V4 Flash", OpenCodeGo, OpenCodeGo, 1_000_000;
    Gpt41, "gpt-4.1", "GPT-4.1", OpenAi, OpenAi, 1_047_576;
    Gpt41Mini, "gpt-4.1-mini", "GPT-4.1 Mini", OpenAi, OpenAi, 1_047_576;
    Gpt41Nano, "gpt-4.1-nano", "GPT-4.1 Nano", OpenAi, OpenAi, 1_047_576;
    Gpt4o, "gpt-4o", "GPT-4o", OpenAi, OpenAi, 128_000;
    Gpt4oMini, "gpt-4o-mini", "GPT-4o Mini", OpenAi, OpenAi, 128_000;
    DeepSeekChat, "deepseek-chat", "DeepSeek Chat (Native)", DeepSeek, DeepSeek, 1_000_000;
    DeepSeekReasoner, "deepseek-reasoner", "DeepSeek Reasoner (Native)", DeepSeek, DeepSeek, 1_000_000;
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

/// Persistence namespaces for dynamic provider model identities.
pub const OPENCODE_GO_MODEL_PREFIX: &str = "opencode-go::";
pub const OPENCODE_ZEN_MODEL_PREFIX: &str = "opencode-zen::";
pub const OPENAI_MODEL_PREFIX: &str = "openai::";
pub const ANTHROPIC_MODEL_PREFIX: &str = "anthropic::";
pub const DEEPSEEK_MODEL_PREFIX: &str = "deepseek::";
pub const OPENROUTER_MODEL_PREFIX: &str = "openrouter::";

/// A model selected for a session: a legacy built-in or an opaque provider id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModelRef {
    /// A model from Mew's legacy built-in catalog.
    BuiltIn(ModelId),
    /// An exact OpenCode Go model id.
    OpenCodeGo(String),
    /// An exact OpenCode Zen model id.
    OpenCodeZen(String),
    /// An exact native OpenAI model id.
    OpenAi(String),
    /// An exact native Anthropic model id.
    Anthropic(String),
    /// An exact native DeepSeek model id.
    DeepSeek(String),
    /// An exact OpenRouter model id.
    OpenRouter(String),
}

impl ModelRef {
    fn dynamic(
        id: impl Into<String>,
        constructor: impl FnOnce(String) -> Self,
    ) -> Result<Self, ModelRefParseError> {
        let id = id.into();
        if id.is_empty() {
            return Err(ModelRefParseError(id));
        }
        Ok(constructor(id))
    }

    /// Construct an OpenCode Go identity while preserving the exact upstream id.
    pub fn open_code_go(id: impl Into<String>) -> Result<Self, ModelRefParseError> {
        Self::for_provider(ProviderId::OpenCodeGo, id.into())
    }

    /// Construct an OpenCode Zen identity while preserving the exact upstream id.
    pub fn open_code_zen(id: impl Into<String>) -> Result<Self, ModelRefParseError> {
        Self::for_provider(ProviderId::OpenCodeZen, id.into())
    }

    /// Construct an OpenAI identity while preserving the exact upstream id.
    pub fn openai(id: impl Into<String>) -> Result<Self, ModelRefParseError> {
        Self::for_provider(ProviderId::OpenAi, id.into())
    }

    /// Construct an Anthropic identity while preserving the exact upstream id.
    pub fn anthropic(id: impl Into<String>) -> Result<Self, ModelRefParseError> {
        Self::for_provider(ProviderId::Anthropic, id.into())
    }

    /// Construct a DeepSeek identity while preserving the exact upstream id.
    pub fn deepseek(id: impl Into<String>) -> Result<Self, ModelRefParseError> {
        Self::for_provider(ProviderId::DeepSeek, id.into())
    }

    /// Construct an OpenRouter identity while preserving the exact upstream id.
    pub fn openrouter(id: impl Into<String>) -> Result<Self, ModelRefParseError> {
        Self::for_provider(ProviderId::OpenRouter, id.into())
    }

    fn for_provider(provider: ProviderId, id: String) -> Result<Self, ModelRefParseError> {
        if let Some(model) = legacy_model(provider, &id) {
            return Ok(Self::BuiltIn(model));
        }
        match provider {
            ProviderId::OpenCodeGo => Self::dynamic(id, Self::OpenCodeGo),
            ProviderId::OpenCodeZen => Self::dynamic(id, Self::OpenCodeZen),
            ProviderId::OpenAi => Self::dynamic(id, Self::OpenAi),
            ProviderId::Anthropic => Self::dynamic(id, Self::Anthropic),
            ProviderId::DeepSeek => Self::dynamic(id, Self::DeepSeek),
            ProviderId::OpenRouter => Self::dynamic(id, Self::OpenRouter),
        }
    }

    /// Provider responsible for this model.
    pub fn provider(&self) -> ProviderId {
        match self {
            Self::BuiltIn(model) => model.provider(),
            Self::OpenCodeGo(_) => ProviderId::OpenCodeGo,
            Self::OpenCodeZen(_) => ProviderId::OpenCodeZen,
            Self::OpenAi(_) => ProviderId::OpenAi,
            Self::Anthropic(_) => ProviderId::Anthropic,
            Self::DeepSeek(_) => ProviderId::DeepSeek,
            Self::OpenRouter(_) => ProviderId::OpenRouter,
        }
    }

    /// Endpoint protocol spoken by this model.
    pub fn kind(&self) -> ModelKind {
        match self {
            Self::BuiltIn(model) => model.kind(),
            Self::OpenCodeGo(_) => ModelKind::OpenCodeGo,
            Self::OpenCodeZen(_) => ModelKind::OpenCodeZen,
            Self::OpenAi(_) => ModelKind::OpenAi,
            Self::Anthropic(_) => ModelKind::AnthropicMessages,
            Self::DeepSeek(_) => ModelKind::DeepSeek,
            Self::OpenRouter(_) => ModelKind::OpenRouter,
        }
    }

    /// Exact id sent upstream. Persistence namespaces never leak to providers.
    pub fn raw_id(&self) -> &str {
        match self {
            Self::BuiltIn(model) => model.as_str(),
            Self::OpenCodeGo(id)
            | Self::OpenCodeZen(id)
            | Self::OpenAi(id)
            | Self::Anthropic(id)
            | Self::DeepSeek(id)
            | Self::OpenRouter(id) => id,
        }
    }

    /// Human-readable fallback when no runtime catalog name is available.
    pub fn display_name(&self) -> &str {
        match self {
            Self::BuiltIn(model) => model.display_name(),
            Self::OpenCodeGo(id)
            | Self::OpenCodeZen(id)
            | Self::OpenAi(id)
            | Self::Anthropic(id)
            | Self::DeepSeek(id)
            | Self::OpenRouter(id) => id,
        }
    }

    /// Context limit from the legacy catalog or a persisted runtime snapshot.
    pub fn context_limit(&self, snapshot: Option<u64>) -> u64 {
        match self {
            Self::BuiltIn(model) => model.context_limit(),
            Self::OpenCodeGo(_)
            | Self::OpenCodeZen(_)
            | Self::OpenAi(_)
            | Self::Anthropic(_)
            | Self::DeepSeek(_)
            | Self::OpenRouter(_) => snapshot.unwrap_or(0),
        }
    }
}

impl utoipa::ToSchema for ModelRef {}

impl utoipa::PartialSchema for ModelRef {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .into()
    }
}

impl Default for ModelRef {
    fn default() -> Self {
        Self::BuiltIn(ModelId::default())
    }
}

impl From<ModelId> for ModelRef {
    fn from(model: ModelId) -> Self {
        Self::BuiltIn(model)
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(model) => f.write_str(model.as_str()),
            Self::OpenCodeGo(id) => write!(f, "{OPENCODE_GO_MODEL_PREFIX}{id}"),
            Self::OpenCodeZen(id) => write!(f, "{OPENCODE_ZEN_MODEL_PREFIX}{id}"),
            Self::OpenAi(id) => write!(f, "{OPENAI_MODEL_PREFIX}{id}"),
            Self::Anthropic(id) => write!(f, "{ANTHROPIC_MODEL_PREFIX}{id}"),
            Self::DeepSeek(id) => write!(f, "{DEEPSEEK_MODEL_PREFIX}{id}"),
            Self::OpenRouter(id) => write!(f, "{OPENROUTER_MODEL_PREFIX}{id}"),
        }
    }
}

impl FromStr for ModelRef {
    type Err = ModelRefParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        for (prefix, provider) in [
            (OPENCODE_GO_MODEL_PREFIX, ProviderId::OpenCodeGo),
            (OPENCODE_ZEN_MODEL_PREFIX, ProviderId::OpenCodeZen),
            (OPENAI_MODEL_PREFIX, ProviderId::OpenAi),
            (ANTHROPIC_MODEL_PREFIX, ProviderId::Anthropic),
            (DEEPSEEK_MODEL_PREFIX, ProviderId::DeepSeek),
            (OPENROUTER_MODEL_PREFIX, ProviderId::OpenRouter),
        ] {
            if let Some(id) = value.strip_prefix(prefix) {
                return Self::for_provider(provider, id.to_owned());
            }
        }
        ModelId::ALL
            .iter()
            .copied()
            .find(|model| model.as_str() == value)
            .map(Self::BuiltIn)
            .ok_or_else(|| ModelRefParseError(value.to_owned()))
    }
}

impl serde::Serialize for ModelRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for ModelRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Error returned when a persisted model identity is invalid or ambiguous.
#[derive(Debug, thiserror::Error)]
#[error("unsupported model reference: {0}")]
pub struct ModelRefParseError(pub String);

/// Canonical provider metadata shared by server and TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub display_name: &'static str,
    pub env_key: &'static str,
}

/// Providers exposed by `/connect` and `/providers`, in display order.
pub const SUPPORTED_PROVIDERS: &[ProviderDescriptor] = &[
    ProviderDescriptor {
        id: ProviderId::OpenCodeGo,
        display_name: "OpenCode Go",
        env_key: crate::env::OPENCODE_GO_API_KEY,
    },
    ProviderDescriptor {
        id: ProviderId::OpenAi,
        display_name: "OpenAI",
        env_key: crate::env::OPENAI_API_KEY,
    },
    ProviderDescriptor {
        id: ProviderId::DeepSeek,
        display_name: "DeepSeek",
        env_key: crate::env::DEEPSEEK_API_KEY,
    },
    ProviderDescriptor {
        id: ProviderId::OpenRouter,
        display_name: "OpenRouter",
        env_key: crate::env::OPENROUTER_API_KEY,
    },
    ProviderDescriptor {
        id: ProviderId::OpenCodeZen,
        display_name: "OpenCode Zen",
        env_key: crate::env::OPENCODE_ZEN_API_KEY,
    },
    ProviderDescriptor {
        id: ProviderId::Anthropic,
        display_name: "Anthropic",
        env_key: crate::env::ANTHROPIC_API_KEY,
    },
];

/// One model in a provider's runtime registry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
    pub provider: ProviderId,
    pub kind: ModelKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub is_free: bool,
}

impl ModelEntry {
    /// Convert a provider registry row into an unambiguous persisted identity.
    pub fn model_ref(&self) -> Result<ModelRef, ModelRefParseError> {
        ModelRef::for_provider(self.provider, self.id.clone())
    }
}

fn legacy_model(provider: ProviderId, id: &str) -> Option<ModelId> {
    ModelId::ALL
        .iter()
        .copied()
        .find(|model| model.provider() == provider && model.as_str() == id)
}

/// One provider returned by the runtime provider registry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ProviderEntry {
    pub id: ProviderId,
    pub display_name: String,
    pub available: bool,
    pub models: Vec<ModelEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Whether a provider can serve a model using the selected endpoint protocol.
pub fn provider_supports_kind(provider: ProviderId, kind: ModelKind) -> bool {
    matches!(
        (provider, kind),
        (
            ProviderId::OpenCodeGo,
            ModelKind::AnthropicMessages | ModelKind::OpenCodeGo
        ) | (
            ProviderId::OpenCodeZen,
            ModelKind::AnthropicMessages | ModelKind::OpenAiResponses | ModelKind::OpenCodeZen
        ) | (ProviderId::OpenAi, ModelKind::OpenAi)
            | (ProviderId::Anthropic, ModelKind::AnthropicMessages)
            | (ProviderId::DeepSeek, ModelKind::DeepSeek)
            | (ProviderId::OpenRouter, ModelKind::OpenRouter)
    )
}
