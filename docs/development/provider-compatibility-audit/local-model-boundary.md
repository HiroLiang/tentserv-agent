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

The Python model runtime is an internal execution protocol. Rust local-server
adapters should call its runtime routes with native Tentgent request bodies,
even when the client-facing local server route is provider-shaped. Prefer
`/internal/v1/*` aliases where the Python runtime mounts them; otherwise use
the native runtime `/v1/*` route directly.

## Native Route Surface

The local model-bound server surface is capability-driven. The selected local
model or explicit `--capability` decides which endpoint family is valid.

| Capability | Native route family | Provider compatibility role |
| --- | --- | --- |
| `chat` | `POST /v1/chat` | Native fallback for text chat. `POST /v1/chat/completions` is an OpenAI-shaped local ingress adapter. |
| `embedding` | `POST /v1/embeddings` | Native fallback for embedding models. `POST /v1/embeddings` also accepts OpenAI-shaped embedding requests through the local ingress adapter. |
| `rerank` | `POST /v1/rerank` | Native-only; no provider-compatible route today. |
| `vision-chat` | `POST /v1/vision/chat` | Native fallback for local single-image vision chat. |
| `audio-transcription` | `POST /v1/audio/transcriptions` | Native job/runtime route; no provider-compatible transcription route today. |
| `audio-speech` | `POST /v1/audio/speech` | Native job/runtime route; no provider-compatible speech route today. |
| `image-generation` | `POST /v1/images/generations`, `transforms`, `inpaint`, `control` | Native media workflow routes. `POST /v1/images/generations` also accepts OpenAI-shaped text-to-image requests through the local ingress adapter. |
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
proxy edge. The Python runtime still receives native Tentgent payloads. In the
current slice, only implemented local ingress adapters are mounted. Route
families outside the mounted local ingress surface fall through to native
runtime proxy behavior or return native runtime errors.

When a provider-compatible route path collides with a native local route, the
local server must disambiguate by request shape before translating. For example,
bare local embedding requests stay native, while OpenAI-shaped embedding
requests with a string `model` or OpenAI embedding fields enter the OpenAI
adapter.

Current local ingress coverage:

| Provider shape | Local route | Native upstream route | Notes |
| --- | --- | --- | --- |
| OpenAI chat completions | `POST /v1/chat/completions` | `POST /v1/chat` or `POST /v1/chat/stream` | Text-only compatibility. The request `model` is accepted but ignored because the server is already bound to one local model. |
| OpenAI embeddings | `POST /v1/embeddings` | `POST /v1/embeddings` | OpenAI ingress is selected by a string `model` or OpenAI embedding fields. Accepts `input` plus optional `encoding_format: "float"`. The request `model` is accepted but ignored because the server is already bound to one local model. |
| OpenAI image generation | `POST /v1/images/generations` | `POST /v1/images/generations` | OpenAI ingress is selected when `output_path` is absent or OpenAI image fields are present. The adapter accepts `model`, `prompt`, and optional `size`, writes to a runtime-owned output path, and returns an OpenAI-style `b64_json` envelope. |

OpenAI audio route paths collide with native local media routes. Add their
local provider-compatible adapters only with request-shape disambiguation, so
native local media workflows keep their native contracts.

OpenAI local chat accepts text-only chat fields that can map to native local
chat: `messages`, `max_tokens`, `max_completion_tokens`, `temperature`,
`stream`, `response_format: {"type":"text"}`, `modalities: ["text"]`,
`tool_choice: "none"`, `function_call: "none"`, `parallel_tool_calls: false`,
`n: 1`, and `store: false`.

Known unsupported OpenAI fields fail before the Python runtime is called:
tools/function calling, structured response formats, audio output, non-text
content parts, multiple choices, logprobs, web search, provider-side metadata,
cache, safety, and service-tier fields.

OpenAI local embeddings accept `input` as a string or string array and accept
`encoding_format: "float"`. The local adapter rejects `dimensions`,
`encoding_format: "base64"`, `user`, unsupported top-level fields, token-array
inputs, empty input arrays, and empty input strings before the Python runtime is
called. Responses are wrapped into OpenAI-style embedding list envelopes.

OpenAI local image generation accepts `model`, `prompt`, and optional `size`.
The local adapter rejects caller `provider`, `response_format`, `n`, unsupported
top-level fields, empty prompts, and invalid size strings before the Python
runtime is called. The Rust proxy generates the native `output_path`, calls the
Python runtime with a native text-to-image request, reads the generated file,
and returns an OpenAI-style `b64_json` image envelope. Native image generation
requests that include `output_path` remain native and are proxied unchanged.

## Fixture Boundary

Provider compatibility fixtures may test local model-bound servers when the
provider-shaped local ingress behavior itself is the subject under test.

Use local model-bound fixtures for:

- native `/v1/chat`, `/v1/embeddings`, `/v1/rerank`, and `/v1/vision/chat`
  fallback behavior
- implemented provider-shaped local ingress adapters such as OpenAI
  `/v1/chat/completions`, OpenAI `/v1/embeddings`, and OpenAI
  `/v1/images/generations`
- capability gate behavior such as `400 unsupported_provider_capability`
- adapter validation against local chat models
- lower-level runtime streaming contracts

Do not use local model-bound fixtures for:

- direct cloud provider behavior
- provider-shaped route execution that is not implemented on the local server
  yet, such as Claude/Anthropic `/v1/messages` and Gemini
  `/v1beta/models/{operation}`
- unimplemented provider-compatible media behavior such as OpenAI audio,
  Claude/Anthropic media routes, and Gemini media routes

Those tests should target daemon provider-compatible routes, direct cloud
provider servers, or future local ingress adapters when support is implemented.
