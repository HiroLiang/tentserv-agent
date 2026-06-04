# Local Model-Bound Server Boundary

Local model-bound servers are launched with `tentgent server run <model-ref>`.
Their runtime boundary is native Tentgent: provider-shaped requests are
translated at the Rust proxy edge before the Python model runtime sees them.

This boundary exists to keep provider compatibility work from accidentally
turning the Python runtime or native local routes into OpenAI,
Claude/Anthropic, or Gemini APIs. Provider-compatible local routes are ingress
adapters only.

## Launch Shapes

| Launch shape | Server kind | Compatibility classification |
| --- | --- | --- |
| `tentgent server run <model-ref>` | Local model-bound server through the Rust proxy and Python runtime supervisor | Native Tentgent API with provider-shaped ingress adapters where implemented. |
| `tentgent server run <model-ref> --capability <capability>` | Local model-bound server pinned to one local capability family | Native Tentgent API with provider-shaped ingress adapters where implemented. |
| `tentgent server run openai:<model>` | Rust direct cloud provider server | Provider-shaped direct cloud server |
| `tentgent server run anthropic:<model>` or `claude:<model>` | Rust direct cloud provider server | Provider-shaped direct cloud server |
| `tentgent server run gemini:<model>` | Rust direct cloud provider server | Provider-shaped direct cloud server |

Provider-prefixed launch shapes route to cloud provider clients. Local model
refs route to the Python model runtime through native Tentgent request bodies.
When a local provider-shaped ingress route exists, it should be scored as local
provider compatibility, not as direct cloud compatibility.

## Native Route Surface

The local model-bound server surface is capability-driven. The selected local
model or explicit `--capability` decides which endpoint family is valid.

| Capability | Native route family | Provider compatibility role |
| --- | --- | --- |
| `chat` | `POST /v1/chat` | Native fallback for text chat. `POST /v1/chat/completions` is an OpenAI-shaped local ingress adapter. |
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

## Provider-Shaped Local Ingress

Provider-shaped local routes translate request and response bodies at the Rust
proxy edge. The Python runtime still receives native Tentgent payloads.

Current local ingress coverage:

| Provider shape | Local route | Native upstream route | Notes |
| --- | --- | --- | --- |
| OpenAI chat completions | `POST /v1/chat/completions` | `POST /v1/chat` or `POST /v1/chat/stream` | Text-only compatibility. The request `model` is accepted but ignored because the server is already bound to one local model. |

OpenAI local chat accepts text-only chat fields that can map to native local
chat: `messages`, `max_tokens`, `max_completion_tokens`, `temperature`,
`stream`, `response_format: {"type":"text"}`, `modalities: ["text"]`,
`tool_choice: "none"`, `function_call: "none"`, `parallel_tool_calls: false`,
`n: 1`, and `store: false`.

Known unsupported OpenAI fields fail before the Python runtime is called:
tools/function calling, structured response formats, audio output, non-text
content parts, multiple choices, logprobs, web search, provider-side metadata,
cache, safety, and service-tier fields.

## Fixture Boundary

Provider compatibility fixtures may test local model-bound servers when the
provider-shaped local ingress behavior itself is the subject under test.

Use local model-bound fixtures for:

- native `/v1/chat`, `/v1/embeddings`, `/v1/rerank`, and `/v1/vision/chat`
  fallback behavior
- implemented provider-shaped local ingress adapters such as OpenAI
  `/v1/chat/completions`
- capability gate behavior such as `400 unsupported_target`
- adapter validation against local chat models
- lower-level runtime streaming contracts

Do not use local model-bound fixtures for:

- direct cloud provider behavior
- provider-shaped routes that are not implemented on the local server yet, such
  as Claude/Anthropic `/v1/messages` and Gemini `/v1beta/models/{operation}`
- provider-compatible image-generation behavior

Those tests should target daemon provider-compatible routes or direct cloud
provider servers until local ingress support is implemented.
