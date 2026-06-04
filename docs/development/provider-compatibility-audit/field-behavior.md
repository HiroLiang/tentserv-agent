# Shared Field Behavior

This document summarizes fields that appear across provider-compatible routes.
It also separates current `v0.6.0` compatibility audit facts from future runtime
profile work.

## Field Summary

| Field | Current behavior | Notes |
| --- | --- | --- |
| `model` | Required on daemon provider-shaped chat and image routes; accepted by daemon embeddings. Ignored by direct cloud provider servers and local model-bound OpenAI chat ingress. | Direct cloud servers use the bound provider model from `tentgent server run <provider>:<model>`. Local model-bound servers use the bound local model from `tentgent server run <model-ref>`. |
| `model_ref` | Native Tentgent model selector. Accepted by daemon embeddings. Omitted from local model-bound server requests because the server is already bound to one model. | Local model routes use `model_ref`; provider-shaped chat routes use `model`. |
| `messages` | Required by OpenAI/Claude-shaped chat routes. | Daemon and local OpenAI routes are text-first. Direct cloud routes accept more image content. |
| `contents` | Required by Gemini-shaped chat routes. | Daemon route accepts text parts only. Direct cloud route accepts text and `inlineData`. |
| `stream` | Supported by daemon OpenAI/Claude chat and local OpenAI chat ingress. Direct cloud OpenAI uses generic Tentgent SSE; direct cloud Claude ignores it. | Gemini uses route suffix `:streamGenerateContent`, not a body field. |
| `max_tokens` | Forwarded for OpenAI/Claude-shaped chat and native cloud chat. | No route-level model profile or clamp exists yet. |
| `max_completion_tokens` | Accepted by OpenAI-shaped chat and treated as fallback when `max_tokens` is absent. | Only OpenAI-shaped routes read this field. |
| `maxOutputTokens` | Accepted through Gemini `generationConfig`. | No route-level model profile or clamp exists yet. |
| `temperature` | Forwarded for chat routes when present. | Provider/runtime/model limits may still apply. |
| `tools` / function calling | Explicitly rejected on daemon OpenAI/Claude/Gemini routes and direct cloud provider-shaped chat routes. | Stable known-field rejection uses `unsupported_provider_field`. |
| `response_format` | Ignored by daemon/direct image generation handlers when sent by caller. | Cloud client internally omits it for `gpt-image-*` and sends `b64_json` for older OpenAI image models. |
| `dimensions` | Rejected by daemon embeddings as an unknown field; ignored by direct cloud embeddings. | No dimension override is forwarded today. |
| `size` | Accepted by cloud image generation routes. | OpenAI receives `size`; Gemini receives `sampleImageSize`. |
| `voice`, `language`, `output_format` | Not accepted by provider-compatible audio routes because those routes do not exist yet. | Native audio job routes have separate contracts. |

## Required, Optional, Default

Required fields are currently defined by handler request structs and manual
parsers, not by a shared compatibility schema.

Optional fields generally have one of these behaviors:

- route-level default, such as `stream = false`
- no route-level default, passed as absent to the kernel/cloud client
- provider-specific default inside the external provider
- ignored when the current request struct does not include the field

Default values are not model-profile-driven yet. For example, `temperature`,
context length, token limits, embedding dimensions, image sizes, and audio
format recommendations are not adjusted dynamically by model family or model
size in this compatibility layer.

## Model-Specific Parameter Notes

Model-specific recommendations belong to the later runtime profile work, not to
the `v0.6.0` compatibility-contract slice.

Current behavior:

- Chat token and temperature fields are forwarded when accepted by the route.
- Image `size` is forwarded or translated when accepted by the route.
- Embedding `dimensions` is not forwarded.
- Provider or backend errors are the current source of truth when a model does
  not support a forwarded parameter.

Future runtime profiles should define:

- accepted parameters per capability, backend, model family, quantization, and
  platform
- safe defaults
- hard limits
- fields to translate, reject, or drop
- model-specific recommended values for context length, max output tokens,
  temperature/top-p, image size, audio format, precision, and memory-sensitive
  knobs

## Unknown-Field Behavior

Unknown-field behavior is currently inconsistent:

- daemon embeddings reject unknown top-level fields manually
- daemon and direct cloud chat handlers generally ignore unknown fields
- daemon and direct image-generation handlers generally ignore unknown fields
- direct cloud embeddings ignore unknown fields

The `v0.6.0` error-semantics slice stabilizes known unsupported provider fields
as `unsupported_provider_field`, unsupported provider content as
`unsupported_provider_content`, unsupported path operations as
`unsupported_provider_operation`, and provider/capability gaps as
`unsupported_provider_capability`. It does not make every unknown provider field
strict yet.

## Response Shape Notes

- Daemon OpenAI chat returns OpenAI-shaped non-streaming and streaming response
  shapes.
- Daemon Claude chat returns Claude-shaped non-streaming and streaming response
  shapes.
- Daemon Gemini chat returns Gemini-shaped non-streaming and streaming response
  shapes.
- Local model-bound OpenAI chat ingress returns OpenAI-shaped non-streaming and
  streaming response shapes while forwarding native Tentgent chat bodies to the
  Python runtime.
- Direct cloud OpenAI/Gemini streaming currently returns generic Tentgent
  `delta` and `done` SSE events.
- Embedding responses are Tentgent-shaped for both daemon and direct cloud
  routes.
- Image generation responses are OpenAI-like, with `created` and `b64_json`.
- Local model-bound native routes return native Tentgent response shapes. Local
  provider-shaped ingress routes translate native runtime results back into the
  provider response shape at the Rust proxy edge.
