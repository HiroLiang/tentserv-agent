# M6S Media Serving And Runtime Stream Proxy Decision

Status: deferred to post-M7. No M6 implementation.

M6S records the decision that media-serving wrappers and runtime stream proxy
work should not block M7. Long-lived media server routes require a broader
serving architecture because some capabilities need request/response wrapping,
some need durable job workspaces, and some may later need backend-specific
streaming adapters.

## Goal

Preserve a clear future direction without opening partially designed server
surfaces before M7.

After M7, Tentgent may let users start a server for one media capability and
know exactly which route family it serves:

```bash
tentgent server run <model-ref> --capability vision-chat --port 8783
tentgent server run <model-ref> --capability audio-transcription --port 8784
```

For the current M6-to-M7 track, do not add these commands or routes. The
existing direct server remains limited to `chat`, `embedding`, and `rerank`.
Durable artifacts, long-running generation, resumable work, and very large
inputs stay in daemon job workflows.

## Depends On

- [M3 server compatibility gates](./m3-server-compatibility-gates.md)
- [M4 embedding MVP](./m4-embedding-mvp.md)
- [M5 rerank MVP](./m5-rerank-mvp.md)
- [M6C audio transcription daemon MVP](./m6c-audio-transcription-daemon-mvp.md)
- [M6F vision chat image input](./m6f-vision-chat-image-input.md)
- [M6H MLX multimodal backend foundation](./m6h-mlx-multimodal-backend-foundation.md)
- [M6I MLX vision chat backend](./m6i-mlx-vision-chat-backend.md)
- [M6J MLX audio runtime backend](./m6j-mlx-audio-runtime-backend.md)
- [Post-M7 runtime compatibility architecture](./post-m7-runtime-compatibility-architecture.md)

## Current State

- `tentgent server` supports `chat`, `embedding`, and `rerank`.
- Direct server endpoints are served by the Python server runtime:
  - `POST /v1/chat`
  - `POST /v1/embeddings`
  - `POST /v1/rerank`
- The Python server process is capability-specific. A server launched for one
  endpoint family rejects known routes for other families with
  `unsupported_target`.
- Daemon media workflows already exist:
  - synchronous `POST /v1/vision/chat`
  - durable audio transcription jobs
  - durable audio speech jobs
  - durable image-generation/editing jobs
  - durable video-understanding jobs
- M6R intentionally added only an internal video-generation artifact contract.

## Route Matrix

Post-M7 serving work should start from this policy:

| Capability | Direct server | Candidate direct route | Durable job route | M6 decision |
| --- | --- | --- | --- | --- |
| `chat` | implemented | `POST /v1/chat` | no | Keep as-is. |
| `embedding` | implemented | `POST /v1/embeddings` | no | Keep as-is. |
| `rerank` | implemented | `POST /v1/rerank` | no | Keep as-is. |
| `vision-chat` | post-M7 candidate | `POST /v1/vision/chat` | no | Defer serving wrapper. Existing daemon route remains. |
| `audio-transcription` | post-M7 candidate | `POST /v1/audio/transcriptions` | `POST /v1/audio/transcriptions/job` | Defer serving wrapper. Existing job route remains. |
| `audio-speech` | no | none | `POST /v1/audio/speech/job` | Keep as job. |
| `image-generation` | no | none | image job routes | Keep as job. |
| `video-understanding` | no | none | `POST /v1/video/understanding/job` | Keep as job. |
| `video-generation` | no | none | none yet | No public capability or real fixture gate yet. |

## Runtime Stream Proxy Decision

M6S must not add a generic "stream bytes into the backend and stream whatever
comes out" proxy before M7.

Reasons:

- Each media family has different framing and completion semantics.
- Audio, image, and video models commonly need complete decoded inputs, not
  arbitrary incomplete byte chunks.
- Opaque proxying would leak backend-specific APIs and make Tentgent's stable
  API boundary unclear.
- Durable outputs need job workspaces, retention, and cleanup, not a raw server
  socket.

Already allowed streaming:

- Chat text streaming through existing SSE events.
- Result downloads from daemon jobs stream from disk where implemented.

Deferred streaming:

- Realtime audio input/output.
- Live video input.
- Bidirectional sockets.
- Backend-native opaque streaming protocols.
- Media server wrappers that need warm-model request/response wrapping.

## Post-M7 Direct `vision-chat` Server Seed

After M7, add `vision-chat` to `ServerCapability` only inside a focused media
serving wrapper plan. Direct server launch should require a managed model with
`ModelCapability::VisionChat`.

Candidate route:

```http
POST /v1/vision/chat
Content-Type: multipart/form-data
```

Candidate multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | Exactly one image. |
| `prompt` | yes | text | User prompt for the image. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `json`, or `md`; default `text`. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional sampling temperature. |
| `stream` | no | boolean text | Future only, and only if backend supports text streaming. |

The direct server should not accept `model_ref`; the server process already
binds one model at launch. The wrapper boundary must decide whether the Python
server owns request-scoped temp files directly or whether a daemon sidecar
mediates media upload and cleanup.

## Post-M7 Direct `audio-transcription` Server Seed

After M7, add `audio-transcription` to `ServerCapability` only after the
`vision-chat` server path is stable. Launch should require a managed model with
`ModelCapability::AudioTranscription`.

Candidate route:

```http
POST /v1/audio/transcriptions
Content-Type: multipart/form-data
```

Candidate multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `file` | yes | file bytes | Exactly one audio file. |
| `output_format` | no | text | `json`, `text`, `vtt`, or `srt`; default `json` for direct server. |
| `language` | no | text | Optional runtime language hint. |
| `timestamps` | no | boolean text | Required for subtitle formats when backend supports timestamps. |

Large, slow, or durable transcription requests should continue using daemon
jobs.

## Post-M7 Questions

- Should media serving live directly in the Python server process, or should
  the Rust daemon wrap a warm server process and own upload/temp-file handling?
- Should direct media serving be limited to a strict request size and duration,
  with all larger work rejected toward daemon jobs?
- Should direct media serving share runtime code with CLI once-runtimes, or
  load long-lived backend objects to avoid repeated model startup?
- Should future stream proxying be implemented only through typed routes such
  as chat SSE and future speech streaming, rather than a generic tunnel?

## M6 Execution Plan

1. Record the route matrix and the no-opaque-proxy decision.
2. Move media-serving wrappers and runtime stream proxy design to the post-M7
   architecture marker.
3. Do not extend `ServerCapability` in M6.
4. Do not add direct media routes to the Python server in M6.
5. Keep user docs focused on existing daemon media workflows and existing
   `chat`/`embedding`/`rerank` direct server routes.

## Acceptance Criteria

- The route matrix is documented.
- M6 explicitly does not add media direct server routes.
- Post-M7 architecture work tracks media-serving wrappers and runtime stream
  proxy decisions.
- Existing direct server capabilities remain `chat`, `embedding`, and `rerank`.
- Durable artifact workflows remain jobs and are not exposed as direct server
  routes.
- No opaque runtime stream proxy is added before M7.

## Non-Goals

- Generic backend proxying before M7.
- WebSockets or bidirectional sockets before M7.
- Realtime audio or video streaming before M7.
- OpenAI, Claude, Gemini, or other provider-compatible multimodal routes.
- Direct server routes for image generation, audio speech, video understanding,
  or video generation.
- Persisting direct server requests as jobs.
