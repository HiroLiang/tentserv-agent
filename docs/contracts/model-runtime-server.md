# Model Runtime Server

This document defines shared lifecycle behavior for the direct Python model
runtime daemon.

## Capabilities

One Python runtime process serves one endpoint family. Rust chooses the
capability when it starts the process through the runtime daemon entrypoint.

Supported capability values:

- `chat`
- `embedding`
- `rerank`
- `audio-transcription`
- `audio-speech`
- `image-generation`
- `lora-tuning`
- `video-understanding`
- `vision-chat`

Capability endpoints:

- `POST /v1/chat`
- `POST /v1/chat/stream`
- `POST /v1/embeddings`
- `POST /v1/rerank`
- `POST /v1/audio/transcriptions`
- `POST /v1/audio/speech`
- `POST /v1/images/generations`
- `POST /v1/images/transforms`
- `POST /v1/images/inpaint`
- `POST /v1/images/control`
- `POST /v1/tuning/lora/runs`
- `POST /v1/video/understanding`
- `POST /v1/vision/chat`

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

### Image Generation

`POST /v1/images/generations` runs text-to-image generation.
`POST /v1/images/transforms` runs image-to-image generation.
`POST /v1/images/inpaint` runs image and mask inpainting.
`POST /v1/images/control` runs Diffusers ControlNet-style controlled
generation.

Supported `model_kind` values:

- `diffusers-text-to-image`
- `diffusers-image-to-image`
- `diffusers-inpaint`
- `diffusers-control`
- `mlx-diffusion-text-to-image`
- `mlx-diffusion-image-to-image`
- `mlx-diffusion-inpaint`

The direct runtime receives local input paths and writes to the provided
`output_path`. Rust remains responsible for job workspaces, upload/download
handling, model and adapter resolution, and route selection. Python validates
the concrete local paths it receives, loads the requested backend model, applies
one optional image LoRA adapter when the backend supports it, and writes one
`png` or `jpg` output.

Control generation requires a resolved ControlNet-style adapter record in the
request. MLX diffusion has no control route because the current MFLUX-backed
runtime does not provide a compatible ControlNet API.

### LoRA Tuning

`POST /v1/tuning/lora/runs` runs one local chat / causal-LM LoRA tuning job and
returns the final adapter output path plus parsed backend events.

Supported `backend` values:

- `peft`
- `mlx`

PEFT tuning requires a `safetensors` chat model and uses Transformers plus PEFT
with `AutoModelForCausalLM`. MLX tuning requires an `mlx` chat model and shells
out to `mlx_lm.lora` with a generated config. The direct runtime validates the
local dataset directory, requires `train.jsonl`, renders canonical
`tentgent.chat.v1` records, and writes backend outputs under the provided
`output_dir`.

This direct endpoint does not create managed train plans, durable run records,
or adapter-store imports. Rust remains responsible for managed model, dataset,
adapter, and workspace resolution before it calls the runtime.

### Vision Chat

`POST /v1/vision/chat` runs one local image-plus-prompt request and returns
text.

Supported `model_kind` values:

- `transformers-image-text-to-text`
- `mlx-vlm`

The direct runtime receives a local `image_path`; Rust remains responsible for
multipart uploads, job workspaces, model resolution, and server selection. Python
validates the concrete local path, loads the requested backend model, and
returns `text`, `json`, or `md` text output with a media type and finish reason.

### Video Understanding

`POST /v1/video/understanding` runs one local video-plus-prompt request and
returns text.

Supported `model_kind` values:

- `transformers-video-understanding`
- `mlx-vlm`

The direct runtime receives a local `video_path`; Rust remains responsible for
multipart uploads, job workspaces, model resolution, and server selection.
Python validates the concrete local path, sampling bounds, optional focus
regions, and optional context text before backend execution.

Transformers video understanding samples bounded frames through OpenCV, passes
the sampled frames as image inputs, and uses the prompt, system prompt, focus
regions, transcript, and context notes as text guidance. MLX VLM video
understanding uses the `mlx-vlm` video preprocessing path only for known
video-capable model types. Unsupported MLX model types return `501` with a
machine-readable `mlx_video_model_unsupported` detail containing
`supported_model_types`.

## Dependency Profiles

The Python project exposes an `audio` optional dependency group for local audio
runtime support. The broader `local-model` group includes audio dependencies
alongside chat, embedding, rerank, and image dependencies. The `image` optional
dependency group installs Diffusers, Pillow, PyTorch, Transformers/Safetensors,
and Apple Silicon MFLUX/MLX packages where supported. The `vision` optional
dependency group installs Transformers, Pillow, PyTorch/Torchvision, and Apple
Silicon MLX VLM packages where supported. Video understanding also requires
OpenCV-backed video decoding through the `vision` or `local-model` dependency
profile. The `training` optional dependency group installs Transformers, PEFT,
PyTorch, and Apple Silicon MLX LoRA packages where supported.

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
