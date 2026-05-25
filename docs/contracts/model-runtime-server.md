# Model Runtime Server

This document defines shared lifecycle behavior for the direct Python model
runtime server.

## Capabilities

One Python runtime process serves one endpoint family. Rust chooses the
capability when it starts the process through the server CLI.

Supported capability values:

- `chat`
- `embedding`
- `rerank`
- `audio-transcription`
- `audio-speech`

Capability endpoints:

- `POST /v1/chat`
- `POST /v1/chat/stream`
- `POST /v1/embeddings`
- `POST /v1/rerank`
- `POST /v1/audio/transcriptions`
- `POST /v1/audio/speech`

Requests to endpoint families not served by the current process return `400`.
Rust still owns job creation, workspace paths, model resolution, and server
selection. The Python runtime only loads the selected model, runs inference, and
returns or writes the prepared result.

### Audio Transcription

`POST /v1/audio/transcriptions` runs batch local audio transcription and writes
the transcript to the provided output path.

Supported `model_kind` values:

- `transformers-asr`
- `mlx-audio`

Supported output formats are `text`, `json`, `vtt`, and `srt`. Subtitle formats
require timestamp chunks from the backend result. `input_path` must exist on the
local filesystem. The response includes output format, media type, output path,
byte count, and best-effort plain text.

### Audio Speech

`POST /v1/audio/speech` runs batch local text-to-speech and writes WAV output to
the provided output path.

Supported `model_kind` values:

- `transformers-tts`
- `mlx-audio`

The direct runtime currently supports `wav` output. `mlx-audio` text-to-speech
uses the optional `mlx-audio` TTS loader when that dependency is installed.
Selected models may reject `voice` or `language` options when their generated
API does not support them. Kokoro-family MLX TTS models also require the
`misaki[en]` optional dependency for English grapheme-to-phoneme processing.

## Dependency Profiles

The Python project exposes an `audio` optional dependency group for local audio
runtime support. The broader `local-model` group includes audio dependencies
alongside chat, embedding, and rerank dependencies.

## Health

`GET /healthz` returns the runtime process snapshot. Rust uses this endpoint to
distinguish ready, closing, and shutdown states for one Python runtime process.

Response fields include:

- `status`: `ok`, `closing`, or `shutdown`
- `pid`
- `server.host`, `server.port`, and optional `server.server_ref`
- `runtime.capability`
- `runtime.model_ref`
- `runtime.resources`
- `tasks`

## Shutdown

`POST /v1/lifecycle/shutdown` requests graceful shutdown of this Python runtime
process.

Behavior:

- the task manager enters `closing`
- new inference requests are rejected
- existing active tasks may finish
- after active tasks finish and the configured closing grace elapses, the
  runtime asks the process host to exit
- resource cleanup still runs through the server lifespan shutdown hook

This endpoint is local to one Python runtime process. Rust daemon process
shutdown remains `POST /v1/daemon/shutdown`; daemon job and server management
remain under `/v1/jobs` and `/v1/servers`.
