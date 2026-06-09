# Provider API Compatibility

This matrix shows which OpenAI, Claude/Anthropic, and Gemini-shaped APIs are
safe to call through Tentgent today.

For copy-paste requests and SDK base URL examples, see
[provider-compatible-examples.md](./provider-compatible-examples.md).

Status key:

- Supported: documented route and request shape are available.
- Partial: the route exists, but Tentgent intentionally supports a narrower
  subset than the upstream provider API.
- Planned: future adapter work is expected, but callers should not depend on it
  yet.
- Unsupported: no compatible route exists, or the provider field or endpoint
  family is not supported and should not be relied on.

## Route Surfaces

| Surface | How to use it | Provider-shaped routes | Notes |
| --- | --- | --- | --- |
| Daemon compatibility adapters | `tentgent daemon start --host 127.0.0.1 --port 8790` | `/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{model}:generateContent`, `/v1beta/models/{model}:streamGenerateContent?alt=sse`, `/v1/embeddings`, `/v1/images/generations` | Daemon provider compatibility routes translate into Tentgent execution. They are intentionally narrower than full provider APIs. |
| Direct cloud provider server | `tentgent server run openai:<model>`, `anthropic:<model>` / `claude:<model>`, or `gemini:<model>` | `/v1/chat`, `/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{operation}`, `/v1/embeddings`, `/v1/images/generations` | A server is bound to one provider model. Unsupported provider/capability combinations fail at server creation or request time. |
| Local model-bound server | `tentgent server run <model-ref>` | `/v1/chat`, `/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{operation}`, `/v1/embeddings`, `/v1/images/generations`, and native capability routes | Native routes stay Tentgent-shaped. Implemented provider-shaped ingress routes translate requests into native local runtime calls and wrap responses back into the provider shape. |

## Endpoint Families

| Endpoint family | OpenAI-compatible | Claude/Anthropic-compatible | Gemini-compatible | Native Tentgent fallback |
| --- | --- | --- | --- | --- |
| Chat | Partial. `/v1/chat/completions` supports text chat fields on the daemon and local model-bound servers; direct OpenAI cloud servers can also translate OpenAI image parts and non-streaming audio chat for compatible models. | Partial. `/v1/messages` supports text Claude-style messages on daemon and local model-bound servers; direct cloud servers can translate text and base64 image blocks for compatible models. | Partial. `generateContent` supports text Gemini-style contents on daemon and local model-bound servers; direct cloud servers can translate text and inline image data for compatible models. | `/v1/chat` for native local text chat. |
| Streaming chat | Partial. `stream: true` returns an SSE wrapper for OpenAI-style chat on daemon and local model-bound OpenAI routes. | Partial. Daemon and local model-bound `/v1/messages` accept streaming chat; the direct cloud server currently rejects `stream: true` and returns only non-streaming Claude-style messages. | Partial. Use `streamGenerateContent?alt=sse`; streaming is route-based rather than a `stream` field and is supported by daemon, direct cloud, and local model-bound Gemini routes. | `/v1/chat` with `stream: true`. |
| Embeddings | Partial. Daemon `/v1/embeddings` can route OpenAI cloud embeddings through `model`; OpenAI-compatible requests return OpenAI-style embedding lists. Direct OpenAI cloud provider servers expose `/v1/embeddings` when the selected model supports embeddings. Local embedding servers accept OpenAI-shaped `/v1/embeddings` requests and return OpenAI-style embedding lists. | Unsupported. Anthropic does not provide a native embedding model through Tentgent today. Voyage AI is not part of Claude compatibility unless added later as a separate provider family. | Partial. Daemon `/v1/embeddings` can route Gemini cloud embeddings through `model` / `provider`; direct cloud provider servers expose `/v1/embeddings` for Gemini embedding models, currently with Tentgent-shaped embedding responses. | `/v1/embeddings` for local embedding models. |
| Rerank | Unsupported as provider-compatible API. | Unsupported. | Unsupported. | `/v1/rerank` for local rerank models. |
| Image generation | Partial. `/v1/images/generations` accepts `model`, `prompt`, and optional `size` on daemon, direct cloud, and image-generation local model-bound servers. | Unsupported. Anthropic image generation is not implemented. | Partial. `/v1/images/generations` can route Gemini image generation requests where the provider model supports it. | `/v1/images/generations/job` for daemon local image-generation workflows; local model-bound image-generation servers also accept native `/v1/images/generations` bodies with `output_path`. |
| Audio transcription | Partial. Direct OpenAI cloud chat accepts `input_audio`; OpenAI-shaped `/v1/audio/transcriptions` is not implemented, and daemon/local chat still reject `input_audio`. | Unsupported. | Unsupported. | `/v1/audio/transcriptions/job`. |
| Audio speech | Partial. Direct OpenAI cloud chat accepts non-streaming `audio` output options; OpenAI-shaped `/v1/audio/speech` is not implemented, and daemon/local chat still reject audio output options. | Unsupported. | Unsupported. | `/v1/audio/speech/job`. |
| Vision chat | Partial. Direct cloud chat servers can pass image parts to compatible OpenAI models; daemon and local model-bound OpenAI chat compatibility remain text-only. | Partial. Direct cloud chat servers can pass base64 image blocks to compatible Claude models; daemon chat compatibility remains text-only. | Partial. Direct cloud chat servers can pass inline image data to compatible Gemini models; daemon chat compatibility remains text-only. | `/v1/vision/chat` for one local image plus one prompt. |
| Media workflows | Unsupported as provider-compatible APIs. | Unsupported as provider-compatible APIs. | Unsupported as provider-compatible APIs. | Use native job routes for image transform, image inpaint, image control, video understanding, audio transcription, and audio speech. |

Tools/function calling and broader provider-compatible multimodal adapters are
planned adapter work. Current callers should treat unsupported fields as hard
limits rather than optional provider pass-through.

## Request Fields

| Field | OpenAI-compatible | Claude/Anthropic-compatible | Gemini-compatible | Notes |
| --- | --- | --- | --- | --- |
| `model` | Supported by daemon adapters. Direct cloud and local model-bound servers are already bound to a model. | Supported by daemon adapters. Direct cloud and local model-bound servers are already bound to a model. | Supported in the path as `{model}` for daemon Gemini routes. Direct cloud and local model-bound servers are already bound to a model. | Native local routes usually use `model_ref`; local OpenAI chat, Claude messages, Gemini generateContent, embedding, and image-generation ingress ignore caller-supplied `model`. |
| `messages` / `contents` | Partial. Text messages are safe on daemon and local model-bound OpenAI routes; image parts require a direct cloud server and a compatible model. | Partial. Text messages are safe on daemon and local model-bound Claude routes; base64 image blocks require a direct cloud server and a compatible model. | Partial. Text parts are safe on daemon and local model-bound Gemini routes; inline image data requires a direct cloud server and a compatible model. | Daemon and local chat compatibility routes do not support unsupported non-text parts. |
| `max_tokens` / `max_completion_tokens` / `maxOutputTokens` | Supported for chat. | Supported for chat as required `max_tokens` on Claude-shaped routes. | Supported through `generationConfig.maxOutputTokens`. | Native chat uses `max_tokens`. |
| `temperature` | Supported for chat. | Supported for chat. | Supported through `generationConfig.temperature`. | Runtime/model limits may still apply. |
| `stream` | Supported for OpenAI-style chat. | Partial. Supported by daemon and local model-bound `/v1/messages`; direct cloud Claude rejects `stream: true`. | Not a body field. Use `streamGenerateContent?alt=sse`. | Native chat supports `stream: true`. |
| `input_audio` | Partial. Supported only by direct OpenAI cloud chat with `wav` or `mp3`; daemon and local model-bound OpenAI chat reject it. | Unsupported. | Unsupported. | Use native audio transcription job routes for local audio workflows. |
| `audio` / non-text `modalities` | Partial. Supported only by non-streaming direct OpenAI cloud chat; daemon and local model-bound OpenAI chat reject them. | Unsupported. | Unsupported. | Use native audio speech job routes for local speech workflows. |
| `tools` / function calling | Planned, currently unsupported. | Planned, currently unsupported. | Planned, currently unsupported. | Current compatibility adapters do not support tool fields; callers should not rely on provider pass-through. |
| `response_format` | Partial. OpenAI chat accepts `{ "type": "text" }`; structured response formats are unsupported. Image generation rejects caller-supplied `response_format` and always returns base64 JSON data. | Unsupported. | Unsupported. | `gpt-image-1` routing intentionally omits the legacy OpenAI `response_format` field. |
| `dimensions` | Unsupported in provider-compatible requests. | Unsupported. | Unsupported in provider-compatible requests. | Local and daemon embeddings do not expose a dimension override. |
| `encoding_format` | Partial. OpenAI-compatible embeddings accept `float`; `base64` is unsupported. | Unsupported. | Unsupported in provider-compatible requests. | Embedding responses return float arrays. |
| Image size | Supported as `size` on provider-compatible image generation. | Unsupported. | Supported as `size` where the Gemini image model accepts it. | Native local image jobs use `width` and `height`; local OpenAI image ingress maps `size` to `width` and `height`. |
| `voice` | Unsupported in provider-compatible speech routes. | Unsupported. | Unsupported. | Native local speech jobs accept `voice` when the selected model supports it. |
| `language` | Unsupported in provider-compatible audio routes. | Unsupported. | Unsupported. | Native local audio jobs accept `language` when the selected model supports it. |
| `output_format` | Unsupported in provider-compatible audio routes. | Unsupported. | Unsupported. | Native local audio and image jobs expose format-specific `output_format` fields. |

When in doubt, prefer the native Tentgent endpoint family for local models and
use provider-shaped routes only for the rows marked Supported or Partial above.
