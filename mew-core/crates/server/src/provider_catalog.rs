//! Runtime model discovery for supported providers.

use std::collections::HashMap;

use futures::StreamExt;
use mewcode_protocol::{ModelEntry, ModelId, ModelKind, ProviderId};
use serde::Deserialize;

pub const OPENAI_MODELS_URL: &str = "https://api.openai.com/v1/models";
pub const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
pub const OPENCODE_ZEN_MODELS_URL: &str = "https://opencode.ai/zen/v1/models";
pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";
pub const DEEPSEEK_MODELS_URL: &str = "https://api.deepseek.com/models";
pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
pub const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RemoteModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    context_length: Option<u64>,
}

#[doc(hidden)]
pub fn parse_models(body: &str, provider: ProviderId) -> Result<Vec<ModelEntry>, String> {
    if body.len() > MAX_CATALOG_BYTES {
        return Err(format!("{provider} model catalog is too large"));
    }
    let response: ModelsResponse = serde_json::from_str(body)
        .map_err(|error| format!("invalid {provider} model catalog: {error}"))?;
    Ok(response
        .data
        .into_iter()
        .filter_map(|row| serde_json::from_value::<RemoteModel>(row).ok())
        .filter(|model| {
            valid_model_id(&model.id)
                && (provider != ProviderId::OpenAi || !is_openai_non_chat(&model.id))
        })
        .map(|model| normalize_model(model, provider))
        .collect())
}

fn valid_model_id(id: &str) -> bool {
    id == id.trim()
        && !id.is_empty()
        && !id
            .chars()
            .any(|character| character.is_control() || is_bidi_control(character))
}

fn is_openai_non_chat(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    id.contains("embedding")
        || id.contains("moderation")
        || id.starts_with("gpt-image")
        || id.starts_with("dall-e")
        || id.starts_with("tts-")
        || id.starts_with("whisper-")
        || id.contains("transcribe")
        || id.contains("audio")
        || id.contains("realtime")
        || id.starts_with("sora-")
        || matches!(id.as_str(), "babbage-002" | "davinci-002")
        || id.starts_with("computer-use-")
        || id.starts_with("codex-mini-")
}

fn normalize_model(model: RemoteModel, provider: ProviderId) -> ModelEntry {
    let legacy = ModelId::ALL
        .iter()
        .copied()
        .find(|known| known.provider() == provider && known.as_str() == model.id);
    let display_name = model
        .display_name
        .as_deref()
        .or(model.name.as_deref())
        .map(sanitize_name)
        .filter(|name| !name.is_empty())
        .or_else(|| legacy.map(|known| known.display_name().to_owned()))
        .unwrap_or_else(|| sanitize_name(&model.id));
    ModelEntry {
        is_free: provider == ProviderId::OpenRouter && model.id.ends_with(":free"),
        id: model.id,
        display_name,
        provider,
        kind: legacy.map_or_else(|| default_kind(provider), ModelId::kind),
        context_length: model
            .context_length
            .or_else(|| legacy.map(ModelId::context_limit)),
    }
}

fn default_kind(provider: ProviderId) -> ModelKind {
    match provider {
        ProviderId::OpenCodeGo => ModelKind::OpenCodeGo,
        ProviderId::OpenCodeZen => ModelKind::OpenCodeZen,
        ProviderId::OpenAi => ModelKind::OpenAi,
        ProviderId::Anthropic => ModelKind::AnthropicMessages,
        ProviderId::DeepSeek => ModelKind::DeepSeek,
        ProviderId::OpenRouter => ModelKind::OpenRouter,
    }
}

#[derive(Deserialize)]
struct ModelsDevProvider {
    #[serde(default)]
    models: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct ModelsDevModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    limit: ModelsDevLimit,
    #[serde(default)]
    provider: ModelsDevModelProvider,
}

#[derive(Default, Deserialize)]
struct ModelsDevLimit {
    #[serde(default)]
    context: Option<u64>,
}

#[derive(Default, Deserialize)]
struct ModelsDevModelProvider {
    #[serde(default)]
    npm: Option<String>,
}

/// Parse Anthropic's native model catalog using its display names.
#[doc(hidden)]
pub fn parse_anthropic_models(body: &str) -> Result<Vec<ModelEntry>, String> {
    parse_models(body, ProviderId::Anthropic)
}

/// Join Zen's live model ids with Models.dev's explicit transport metadata.
#[doc(hidden)]
pub fn parse_zen_models(live_body: &str, metadata_body: &str) -> Result<Vec<ModelEntry>, String> {
    if live_body.len() > MAX_CATALOG_BYTES || metadata_body.len() > MAX_CATALOG_BYTES {
        return Err("OpenCode Zen model catalog is too large".to_owned());
    }
    let live: ModelsResponse = serde_json::from_str(live_body)
        .map_err(|error| format!("invalid OpenCode Zen model catalog: {error}"))?;
    let providers: HashMap<String, ModelsDevProvider> = serde_json::from_str(metadata_body)
        .map_err(|error| format!("invalid Models.dev catalog: {error}"))?;
    let metadata = providers
        .get("opencode")
        .ok_or_else(|| "Models.dev catalog has no OpenCode provider".to_owned())?;
    let live_ids = live.data.into_iter().filter_map(|row| {
        let model = serde_json::from_value::<RemoteModel>(row).ok()?;
        valid_model_id(&model.id).then_some(model.id)
    });
    let models = live_ids
        .filter_map(|id| {
            let model =
                serde_json::from_value::<ModelsDevModel>(metadata.models.get(&id)?.clone()).ok()?;
            (model.id == id).then_some((id, model))
        })
        .filter_map(|(id, model)| {
            let package = model.provider.npm.as_deref()?;
            let kind = match package {
                "@ai-sdk/anthropic" => ModelKind::AnthropicMessages,
                "@ai-sdk/openai" => ModelKind::OpenAiResponses,
                "@ai-sdk/openai-compatible" => ModelKind::OpenCodeZen,
                _ => return None,
            };
            Some(ModelEntry {
                display_name: model
                    .name
                    .as_deref()
                    .map(sanitize_name)
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| sanitize_name(&id)),
                id,
                provider: ProviderId::OpenCodeZen,
                kind,
                context_length: model.limit.context,
                is_free: false,
            })
        })
        .collect::<Vec<_>>();
    Ok(models)
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_control() || is_bidi_control(character) {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

pub async fn discover_models(
    provider: ProviderId,
    url: &str,
    api_key: &str,
) -> Result<Vec<ModelEntry>, String> {
    let client = reqwest::Client::builder()
        .timeout(DISCOVERY_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            tracing::warn!(
                %provider,
                timeout = error.is_timeout(),
                connect = error.is_connect(),
                "failed to build model discovery client"
            );
            "model catalog unavailable".to_owned()
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
        tracing::warn!(
            %provider,
            timeout = error.is_timeout(),
            connect = error.is_connect(),
            "model discovery request failed"
        );
        if error.is_timeout() {
            "model discovery timed out".to_owned()
        } else {
            "model catalog unavailable".to_owned()
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        tracing::warn!(%provider, %status, "model discovery returned an error status");
        return Err("model catalog unavailable".to_owned());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            tracing::warn!(
                %provider,
                timeout = error.is_timeout(),
                body = error.is_body(),
                decode = error.is_decode(),
                "failed to read model catalog"
            );
            if error.is_timeout() {
                "model discovery timed out".to_owned()
            } else {
                "model catalog unavailable".to_owned()
            }
        })?;
        if body.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
            return Err("model catalog is too large".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    let body = std::str::from_utf8(&body).map_err(|error| {
        tracing::warn!(%provider, %error, "model catalog was not UTF-8");
        "invalid model catalog".to_owned()
    })?;
    let parsed = if provider == ProviderId::Anthropic {
        parse_anthropic_models(body)
    } else {
        parse_models(body, provider)
    };
    parsed.map_err(|error| {
        tracing::warn!(%provider, %error, "failed to parse model catalog");
        if error.contains("too large") {
            "model catalog is too large".to_owned()
        } else {
            "invalid model catalog".to_owned()
        }
    })
}

/// Discover Zen's live catalog and intersect it with Models.dev metadata.
pub async fn discover_zen_models(
    live_url: &str,
    metadata_url: &str,
    api_key: &str,
) -> Result<Vec<ModelEntry>, String> {
    let client = reqwest::Client::builder()
        .timeout(DISCOVERY_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "model catalog unavailable".to_owned())?;
    let (live, metadata) = tokio::try_join!(
        fetch_catalog_body(
            client.get(live_url).bearer_auth(api_key),
            ProviderId::OpenCodeZen
        ),
        fetch_catalog_body(client.get(metadata_url), ProviderId::OpenCodeZen),
    )?;
    parse_zen_models(&live, &metadata).map_err(|error| {
        tracing::warn!(%error, "failed to parse OpenCode Zen catalogs");
        "invalid model catalog".to_owned()
    })
}

async fn fetch_catalog_body(
    request: reqwest::RequestBuilder,
    provider: ProviderId,
) -> Result<String, String> {
    let response = request.send().await.map_err(|error| {
        tracing::warn!(%provider, timeout = error.is_timeout(), "model discovery request failed");
        if error.is_timeout() {
            "model discovery timed out".to_owned()
        } else {
            "model catalog unavailable".to_owned()
        }
    })?;
    if !response.status().is_success() {
        tracing::warn!(%provider, status = %response.status(), "model discovery returned an error status");
        return Err("model catalog unavailable".to_owned());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "model catalog unavailable".to_owned())?;
        if body.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
            return Err("model catalog is too large".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| "invalid model catalog".to_owned())
}
