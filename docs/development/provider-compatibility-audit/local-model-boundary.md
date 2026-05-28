# Local Model-Bound Server Boundary

Local model-bound servers are launched with `tentgent server run <model-ref>`.
They are native Tentgent servers, even when they expose route names that look
similar to provider APIs.

This boundary exists to keep provider compatibility work from accidentally
counting native local runtime routes as OpenAI, Claude/Anthropic, or
Gemini-compatible behavior.

## Launch Shapes

| Launch shape | Server kind | Compatibility classification |
| --- | --- | --- |
| `tentgent server run <model-ref>` | Local model-bound server through the Rust proxy and Python runtime supervisor | Native Tentgent API |
| `tentgent server run <model-ref> --capability <capability>` | Local model-bound server pinned to one local capability family | Native Tentgent API |
| `tentgent server run openai:<model>` | Rust direct cloud provider server | Provider-shaped direct cloud server |
| `tentgent server run anthropic:<model>` or `claude:<model>` | Rust direct cloud provider server | Provider-shaped direct cloud server |
| `tentgent server run gemini:<model>` | Rust direct cloud provider server | Provider-shaped direct cloud server |

Only the provider-prefixed launch shapes belong in provider compatibility
scoring. Local model refs belong in native fallback docs and tests.

## Native Route Surface

The local model-bound server surface is capability-driven. The selected local
model or explicit `--capability` decides which endpoint family is valid.

| Capability | Native route family | Provider compatibility role |
| --- | --- | --- |
| `chat` | `POST /v1/chat` | Native fallback for text chat. |
| `embedding` | `POST /v1/embeddings` | Native fallback for embedding models. |
| `rerank` | `POST /v1/rerank` | Native-only; no provider-compatible route today. |
| `vision-chat` | `POST /v1/vision/chat` | Native fallback for local single-image vision chat. |
| `audio-transcription` | `POST /v1/audio/transcriptions` | Native job/runtime route; no provider-compatible transcription route today. |
| `audio-speech` | `POST /v1/audio/speech` | Native job/runtime route; no provider-compatible speech route today. |
| `image-generation` | `POST /v1/images/generations`, `transforms`, `inpaint`, `control` | Native media workflow routes; provider-compatible image generation is a separate adapter surface. |
| `video-understanding` | `POST /v1/video/understanding` | Native media workflow route. |

Lower-level model-runtime contracts also mention `POST /v1/chat/stream`, but
provider compatibility docs should prefer the stable user-facing chat request
shape: `POST /v1/chat` with `stream: true`.

## Local Chat Example

Required:

- `messages`

Optional:

- `max_tokens`
- `temperature`
- `adapter_ref`
- `stream`

Defaults:

- `stream`: `false`
- bound local model from server launch

Example request:

```json
{
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 64,
  "temperature": 0.0,
  "stream": false
}
```

Example response:

```json
{
  "text": "...",
  "finish_reason": "stop",
  "model_ref": "local-model-ref",
  "adapter_ref": null
}
```

This response is native Tentgent shape. It should not be used as evidence that
`/v1/chat` is OpenAI-, Claude-, or Gemini-compatible.

## Fixture Boundary

Provider compatibility fixtures should test local model-bound servers only when
the native fallback behavior itself is the subject under test.

Use local model-bound fixtures for:

- native `/v1/chat`, `/v1/embeddings`, `/v1/rerank`, and `/v1/vision/chat`
  fallback behavior
- capability gate behavior such as `400 unsupported_target`
- adapter validation against local chat models
- lower-level runtime streaming contracts

Do not use local model-bound fixtures for:

- OpenAI `/v1/chat/completions` compatibility
- Claude/Anthropic `/v1/messages` compatibility
- Gemini `/v1beta/models/{operation}` compatibility
- provider-compatible image-generation behavior
- provider-shaped unknown-field semantics

Those tests should target daemon provider-compatible routes or direct cloud
provider servers instead.
