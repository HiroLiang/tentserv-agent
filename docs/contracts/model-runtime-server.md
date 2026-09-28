# Model Runtime Server

This document defines shared lifecycle behavior for the direct Python model
runtime daemon.

## Capabilities

One Python runtime process serves one endpoint family. Rust chooses the
capability when it starts the process through the runtime daemon entrypoint. If
the caller omitted `--capability` for a local model-bound server, Rust infers
that capability from stored model metadata before launching the Rust server
proxy. The proxy then starts or reuses the matching Python runtime through the
shared runtime daemon supervisor at eager startup or on demand for lazy servers.

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

Runtime capability endpoints are internal Rust-to-Python execution routes:

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

Rust local-server adapters call these runtime routes with native Tentgent
request bodies, even when the client-facing local server route is
provider-shaped. These routes are mounted as `/v1/*` today, but they are not
caller-facing API surfaces. If the Python runtime later mounts
`/internal/v1/*` aliases, Rust may prefer those aliases to keep runtime
execution paths visually separate from public provider-compatible API surfaces.

Requests to endpoint families not served by the current process return `400`.
Rust still owns job creation, workspace paths, model resolution, and server
selection. The Python runtime only loads the selected model, runs inference, and
returns or writes the prepared result.

Provider-shaped OpenAI, Claude/Anthropic, and Gemini route coverage is tracked
in the user-facing
[provider compatibility matrix](../user/provider-compatibility.md). That matrix
is the caller-facing source for which provider endpoint families are supported,
partial, planned, or unsupported through Tentgent.

When the Rust local-server proxy asks the supervisor to start this daemon for a
model-bound server request, it passes `--server-ref`, `--model-ref`, `--home`,
one capability value, and the resolved data root through
`TENTGENT_DATA_ROOT`. In that mode, the matching direct server endpoints may
omit the full `model` record and `model_kind`; Python resolves the managed model
from `TENTGENT_DATA_ROOT/models/store`, falling back to
`TENTGENT_HOME/models/store` when no separate data root is configured, and
infers the runtime kind from the stored primary format. Explicit direct-runtime
requests may still pass `model` and `model_kind`. If a model-bound request
includes a different explicit model, the runtime rejects the request instead
of silently switching resources.

Fixed backend-kind inference is available for chat, embedding, rerank, audio,
vision chat, and video understanding. Image generation infers the backend kind
from both the bound model format and the requested image workflow, because
text-to-image, image-to-image, inpaint, and control use different backend
entrypoints. LoRA tuning remains an explicit direct-runtime endpoint because a
training run owns its base model through the tuning payload and managed train
plan.

Image-generation runtimes require lazy loading, for both Diffusers and MLX/MFLUX.
Direct Python CLI invocation must include `--lazy-load`; programmatic
`RuntimeServerConfig` rejects `lazy_load=False` for this capability. This applies
to both bound and unbound runtimes. Startup must not choose a workflow implicitly
or report eager success without loading its pipeline. Workflow-aware eager
preparation remains outside this contract; request-specific loading is unchanged.

### Audio Transcription

`POST /v1/audio/transcriptions` runs batch local audio transcription
and writes the transcript to the provided output path.

Supported `model_kind` values:

- `transformers-asr`
- `mlx-audio`

Supported output formats are `text`, `json`, `vtt`, and `srt`. Subtitle formats
require timestamp chunks from the backend result. `input_path` must exist on the
local filesystem. The response includes output format, media type, output path,
byte count, and best-effort plain text.

### Audio Speech

`POST /v1/audio/speech` runs batch local text-to-speech and writes WAV
output to the provided output path.

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

`POST /v1/tuning/lora/runs` runs one local chat / causal-LM LoRA
tuning job and returns the final adapter output path plus parsed backend
events.

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

`POST /v1/vision/chat` runs one local image-plus-prompt request and
returns text.

Supported `model_kind` values:

- `transformers-image-text-to-text`
- `mlx-vlm`

The direct runtime receives a local `image_path`; Rust remains responsible for
multipart uploads, job workspaces, model resolution, and server selection. Python
validates the concrete local path, loads the requested backend model, and
returns `text`, `json`, or `md` text output with a media type and finish reason.

### Video Understanding

`POST /v1/video/understanding` runs one local video-plus-prompt request
and returns text.

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
Health is observational: it does not refresh model or runtime activity. Rust
startup probes, supervisor polling, CLI inspection, and ownership inspection
therefore cannot keep an otherwise idle runtime alive.

Response fields include:

- `status`: `ok`, `closing`, or `shutdown`
- `pid`
- internal `process_token`, which must match the opaque token supplied by the
  Rust launcher before PID-based process identity is trusted
- top-level `server_ref` and `runtime_home` for daemon/CLI launch verification
- `server.host`, `server.port`, and optional `server.server_ref`; `server.port`
  is the actual bound port passed by Rust after auto-port selection
- `runtime.capability`
- `runtime.model_ref`
- `runtime.model_bound`
- `runtime.lifecycle.runtime_idle_seconds`
- `runtime.lifecycle.model_idle_seconds`
- `runtime.resources`
- `tasks`

Resource entries include `state` (`available`, `invalidated`, or `quarantined`),
`load_error`, and `cleanup_error`. Diagnostics are strings, not retained loader
tracebacks. Quarantined resources are still counted; they are not reported as freed.

## Idle Policies

The runtime has two independent finite clocks:

- `runtime_idle_seconds` defaults to `300`. It begins when startup becomes
  ready, then resets when accepted runtime work completes. Expiry begins
  graceful process shutdown and the lifespan cleanup calls `release_all()`.
- `model_idle_seconds` defaults to `0`. It begins when the final model lease
  completes. Expiry removes the loaded resource and calls the backend's
  `release()` without stopping the process.

The pair must satisfy
`0 <= model_idle_seconds <= runtime_idle_seconds`. Negative values, including
the former `-1` retain-forever sentinel, and non-finite Python values are
rejected before serving. Retained completed-task metadata does not postpone
runtime shutdown, and a model cannot be released while it has an active lease.

The direct Python CLI accepts `--runtime-idle-seconds` and
`--model-idle-seconds`. The older `--idle-keep-alive-seconds` and
`--model-idle-timeout-seconds` names remain deprecated aliases; a canonical and
legacy value supplied together must match. Rust-managed launches use only the
canonical names.

## Managed Preload

`POST /v1/lifecycle/preload` is an internal, model-bound load validation operation.
Its JSON body contains only required, non-empty string `task_ref` and
`process_token` fields. The token must match this runtime's launcher-supplied
generation token; it is an identity check, not authentication. The operation
accepts no caller-selected model/path, backend, workflow, or adapter.

Preload submits a `preload` task through TaskManager and takes an ordinary model
lease for the runtime's bound model/capability. It executes `load()` (or reuses a
loaded resource), requires `is_loaded`, and exits the lease before returning:

```json
{
  "status": "done",
  "task_ref": "preload-task-ref",
  "model_ref": "bound-model-ref",
  "capability": "chat",
  "process_token": "expected-generation-token"
}
```

Completion validates loading, not continued residency: model idle `0` releases
after the final lease; positive idle preserves normal reuse/expiry. Image
generation and LoRA tuning have no fixed supported preload. MLX embedding and
rerank placeholders are also rejected because their `load()` only sets metadata.

Errors use `detail.code` and `detail.message`; accepted tasks also include
`detail.task_ref`. Generation mismatch diagnostics never disclose the actual token.

| HTTP | Code | Meaning |
| --- | --- | --- |
| 422 | `invalid_preload_request` | Missing, null, wrong-type, blank, extra fields, or malformed JSON. |
| 409 | `runtime_generation_mismatch` | Missing runtime token or mismatched expected token. |
| 409 | `runtime_closing`, `preload_task_exists` | Admission closed or task ref already tracked. |
| 400 | `preload_unbound_runtime` | Runtime has no bound model. |
| 501 | `preload_unsupported` | Unsupported capability/backend or unavailable backend dependency. |
| 500 | `preload_failed` | Loading did not produce a loaded model, or loading/cleanup failed. |
| 504 | `preload_wait_timeout` | Internal 300-second HTTP observation budget expired. |

The wait budget is independent of both idle clocks and has no CLI option.
Timeout/cancellation of the HTTP wait does not cancel queued or running native
loading. Accepted work remains active until it actually finishes; completion
restarts runtime idle, and normal lease exit governs model idle. Late errors are
consumed even without a waiting HTTP client. A backend's own timeout is a load
failure (`500`), not observation expiry (`504`).

Failed loading invalidates only that resource and calls its `release()` under
the resource lock. Reserved waiters reject the invalidated object; after the
last reservation exits, a retry creates a fresh object. Inference-body errors
do not invalidate a successfully loaded model. Failed cleanup quarantines the
object and rejects reuse, without global release or shared-runtime termination.
Idle cleanup skips quarantine, but runtime-idle shutdown still runs and retries
cleanup for all resources; one cleanup failure does not skip other resources.

The public Local proxy returns `404` for the entire `/v1/lifecycle` and
`/internal/v1/lifecycle` namespaces, including slash, query, encoded, and
URL-normalized equivalents, before runtime resolution or proof recording.
This blocks existing shutdown as well as preload. The Local worker invokes
preload after the supervisor's existing health/generation check, including on
reuse; managed Python launch itself stays lazy. Rust allows 305 seconds for the
300-second server wait plus transport overhead and validates completion identity.
Only a matching terminal completion opens Local inference admission. Confirmed
accepted-task load errors fail startup and record failed proof; timeout,
transport, generation, and missing-endpoint errors fail startup without such
proof or shared-runtime termination. See [Local readiness](../user/servers.md#local-startup-and-readiness).
Cluster preload integration remains a later #132 slice.

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

Rust records a profile-aware physical runtime generation as `starting`,
`ready`, or `closing`. The first spawner's idle policy remains effective for
that generation. Later callers reuse the same generation and may receive an
idle-policy mismatch diagnostic. A closing generation has a 5-second barrier,
and replacement cannot start until PID plus process token prove termination.
These records are internal lifecycle state described in
[runtime-ownership.md](./runtime-ownership.md).
