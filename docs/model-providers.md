# Model providers

Mew supports OpenCode Go, OpenCode Zen, OpenAI, native Anthropic, DeepSeek, and OpenRouter. OpenCode Go remains the default provider.

## Connect a provider

Set a provider API key in `.env`, or run `/connect` in the TUI. Mew validates connected credentials and stores them in the local Mew config directory.

```dotenv
OPENCODE_GO_API_KEY=
OPENCODE_ZEN_API_KEY=
OPENAI_API_KEY=
ANTHROPIC_API_KEY=
DEEPSEEK_API_KEY=
OPENROUTER_API_KEY=
```

Credential resolution prefers a stored `/connect` credential, then a provider's server-configuration field when available, then its environment variable.

## Runtime model catalogs

Run `/model` to fetch current catalogs and select a model. Mew queries providers at runtime:

- OpenCode Go: `GET /models` on the configured OpenCode Go base URL.
- OpenCode Zen: `GET /models` on the [Zen API](https://opencode.ai/docs/zen/) and an exact-ID intersection with [Models.dev](https://models.dev/).
- OpenAI: `GET https://api.openai.com/v1/models`.
- Anthropic: [`GET /v1/models`](https://docs.anthropic.com/en/api/models-list) with `x-api-key` and `anthropic-version` headers.
- DeepSeek: `GET https://api.deepseek.com/models`.
- OpenRouter: [`GET /api/v1/models`](https://openrouter.ai/docs/api-reference/list-available-models).

Discovery failures are isolated per provider. Reopen `/model` to refresh. Catalogs are fetched only for configured providers; `available` means configured, while an empty catalog plus `error` means the latest discovery failed.

OpenCode Zen live IDs are accepted only when the same ID has explicit, supported transport metadata in Models.dev. `@ai-sdk/anthropic` maps to Messages, `@ai-sdk/openai` maps to Responses, and `@ai-sdk/openai-compatible` maps to Chat Completions. Gemini, unknown packages, missing metadata, metadata-only rows, and malformed rows are omitted. Transport is never inferred from a model name or prefix.

OpenAI rows that clearly belong to non-chat endpoint families are omitted without a static model allowlist. OpenRouter IDs ending in `:free` are marked `[free]`; availability, pricing, and rate limits remain provider-controlled.

`MEWCODE_ENGINE_BASE_URL` is trusted operator configuration for OpenCode Go and may point to a self-hosted endpoint. Discovery does not follow redirects and each response is capped at 16 MiB.

## Session compatibility

Legacy built-in model IDs remain readable and keep their existing routing, context limits, wire values, and default behavior. Legacy built-in identities remain unqualified for compatibility; other dynamically discovered models use provider-qualified persistence identities:

- `opencode-go::<model-id>`
- `opencode-zen::<model-id>`
- `openai::<model-id>`
- `anthropic::<model-id>`
- `deepseek::<model-id>`
- `openrouter::<model-id>`

Mew strips exactly one namespace before inference and preserves the exact upstream ID. Older binaries that predate a dynamic identity cannot open sessions containing it.

Sessions optionally persist `model_kind` independently from model identity. This transport snapshot is authoritative for Zen because its models can use [Messages, Responses, or Chat Completions](https://opencode.ai/docs/zen/). Legacy sessions without the field fall back to the model's historical transport. Replacing a model without new transport and context snapshots clears stale snapshots; title- and mode-only patches preserve them.

Native Anthropic models always use the [Messages API](https://docs.anthropic.com/en/api/messages) and native Anthropic authentication, never Bearer authentication. OpenAI-compatible model lists often omit context limits, so Mew snapshots a limit only when runtime or legacy metadata provides one; otherwise automatic context compaction is disabled for that model.

External material was paraphrased for compliance with licensing restrictions.
