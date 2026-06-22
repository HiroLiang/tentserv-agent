# HTTP API Reference

This page summarizes the user-facing local HTTP API exposed by
`tentgent daemon`. Start the daemon before calling `/v1/*` routes:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
```

`GET /healthz` is always unauthenticated. `/v1/*` routes are protected when a
daemon token is configured; pass it as:

```bash
Authorization: Bearer $TENTGENT_DAEMON_TOKEN
```

Unless noted otherwise, request bodies are JSON and responses are JSON. Errors
use this shape:

```json
{
  "error": "bad_request",
  "message": "human-readable detail"
}
```

Multipart audio/image endpoints use one daemon-wide upload cap for received
file bytes:

- The default is 20 MiB.
- Operators can adjust it with `TENTGENT_MEDIA_UPLOAD_MAX_BYTES` before
  starting the daemon.
- The cap applies to multipart file parts such as `image` on `/v1/vision/chat`
  and `file` on `/v1/audio/transcriptions/job`. JSON-only routes such as
  `/v1/audio/speech/job` use their own request limits.
- Video understanding uses a separate cap because video files are commonly much
  larger. `TENTGENT_VIDEO_UPLOAD_MAX_BYTES` defaults to 512 MiB and applies to
  `file` on `/v1/video/understanding/job`.
- The cap is an HTTP intake guard, not a model context limit. Model-specific
  image size, audio/video duration, token, or memory failures still come from
  the selected runtime.
- When an uploaded audio/image file part exceeds the cap, the daemon returns
  HTTP `413` with `upload_too_large`. When a video file exceeds the video cap,
  the daemon returns `video_upload_too_large`.

Example:

```json
{
  "error": "upload_too_large",
  "message": "`image` upload exceeds the daemon media upload limit of 20971520 bytes; set TENTGENT_MEDIA_UPLOAD_MAX_BYTES to adjust this limit"
}
```

Video example:

```json
{
  "error": "video_upload_too_large",
  "message": "`file` upload exceeds the daemon video upload limit of 536870912 bytes; set TENTGENT_VIDEO_UPLOAD_MAX_BYTES to adjust this limit"
}
```

References such as `model_ref`, `adapter_ref`, `dataset_ref`, `server_ref`, and
`job_id` accept full refs where available; many routes also accept unique short
prefixes.

For the `v0.9.0` stable, experimental, internal, and deprecated surface audit,
see [api-surface-stability.md](../contracts/api-surface-stability.md). The
route tables below describe callable HTTP routes; they do not make every
backend, cleanup, or recovery behavior a final `1.0.0` guarantee.

## Diagnostics

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/healthz` | Process liveness and service identity. |
| `GET` | `/v1/status` | Daemon status and runtime-home summary. |
| `GET` | `/v1/auth` | Local provider auth presence. Does not reveal secrets. |
| `GET` | `/v1/auth/{provider}` | Provider auth presence for `hf`, `openai`, `anthropic`, or `gemini`. |
| `GET` | `/v1/doctor` | Observational runtime and dependency report. |
| `GET` | `/v1/daemon/logs` | Daemon log metadata. |
| `GET` | `/v1/daemon/logs/stdout?tail_bytes=8192` | Daemon stdout tail. |
| `GET` | `/v1/daemon/logs/stderr?tail_bytes=8192` | Daemon stderr tail. |
| `POST` | `/v1/daemon/shutdown` | Ask the daemon process to shut down. |

## Chat

Native Tentgent chat:

```http
POST /v1/chat
Content-Type: application/json
```

```json
{
  "model_ref": "<chat-model-ref>",
  "adapter_ref": "<optional-adapter-ref>",
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 128,
  "temperature": 0.0,
  "stream": false
}
```

`messages[].role` supports `system`, `user`, and `assistant`. `stream=true`
returns Server-Sent Events.

Compatibility adapters route to the same chat execution path and are text-only:

| Method | Path | Request notes |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI-style `model`, `messages`, optional `adapter_ref`, `max_tokens`, `max_completion_tokens`, `temperature`, `stream`. |
| `POST` | `/v1/messages` | Claude-style `model`, `messages`, optional `system`, `adapter_ref`, `max_tokens`, `temperature`, `stream`. |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini-style `contents`, optional `systemInstruction`, `generationConfig`, `adapter_ref`. |
| `POST` | `/v1beta/models/{model}:streamGenerateContent?alt=sse` | Gemini-style streaming response. |

Tools, function calling, audio content, and non-text message parts are rejected
by chat compatibility routes until their corresponding adapters exist. Send
single-image local vision requests through the native Vision Chat endpoint
below.

For provider-shaped route coverage across OpenAI, Claude/Anthropic, and Gemini
APIs, see [provider-compatibility.md](./provider-compatibility.md).
For copy-paste provider-compatible curl and SDK examples, see
[provider-compatible-examples.md](./provider-compatible-examples.md).

## Vision Chat

Native vision chat accepts one image plus one text prompt and returns generated
text in a JSON envelope. It is a bounded synchronous request, not a durable job.

```http
POST /v1/vision/chat
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | Exactly one image. The daemon does not receive or trust the client's local path. |
| `model_ref` | yes | text | Local `vision-chat` model ref or unique alias. |
| `prompt` | yes | text | User prompt for the image. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `json`, or `md`; defaults to `text`. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional sampling temperature. |

Accepted image media types are `image/png`, `image/jpeg`, and `image/webp`.
The daemon writes uploaded bytes to a request-scoped temp file and removes it
after success or failure; the selected runtime sees a complete image file. The
daemon-wide media upload cap applies to the `image` file part.

```bash
curl -sS http://127.0.0.1:8790/v1/vision/chat \
  -F model_ref=<vision-chat-model-ref> \
  -F prompt='Describe this image in one sentence.' \
  -F output_format=text \
  -F image=@/absolute/path/image.png
```

Response:

```json
{
  "model_ref": "<vision-chat-model-ref>",
  "output_format": "text",
  "text": "A generated answer about the image.",
  "finish_reason": "stop"
}
```

OpenAI, Claude, and Gemini compatible multimodal payloads are not accepted yet.
Those adapters should map into this native vision contract in a later slice.

## Video Understanding Jobs

Canonical video understanding uses a workflow job:

```http
POST /v1/video/understanding/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `file` | yes | file bytes | Video bytes. The daemon does not receive or trust the client's local path. |
| `model_ref` | yes | text | Local `video-understanding` model ref or unique alias. |
| `prompt` | yes | text | User prompt for the video. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `json`, or `md`; defaults to `text`. |
| `output_filename` | no | text | File name only, not a path. Defaults to `video-understanding.<format>`. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional sampling temperature. |
| `sample_fps` | no | float text | Frame sampling rate. Defaults to 1.0; valid range is 0.1..4.0. |
| `max_frames` | no | integer text | Sampled frame cap. Defaults to 32; valid range is 1..128. |
| `max_frame_edge` | no | integer text | Resize sampled frames by largest edge. Defaults to 768; valid range is 128..1536. |
| `clip_start_seconds` | no | float text | Optional non-negative clip start offset. |
| `clip_duration_seconds` | no | float text | Optional positive clip duration. |

`file` must appear exactly once. Send multiple videos as multiple jobs, or
merge them client-side when one combined analysis is intended. The daemon stores
the uploaded bytes in the job workspace, samples bounded frames through the
Python local-model runtime, and then calls the selected
`video-understanding` model. This is not realtime video streaming.

The first runnable baseline samples frames using the local-model Python
runtime's OpenCV-backed decoder. Codec/container support depends on the
packaged OpenCV/FFmpeg build and OS platform. Missing Python decoder packages
and unsupported system codecs fail the job with runtime error details.

`curl` example:

```bash
curl -sS http://127.0.0.1:8790/v1/video/understanding/job \
  -F model_ref=<video-understanding-model-ref> \
  -F prompt='Describe this video briefly.' \
  -F output_format=text \
  -F sample_fps=0.5 \
  -F max_frames=4 \
  -F max_frame_edge=384 \
  -F file=@/absolute/path/video.mp4
```

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "video_understanding",
    "status": "queued",
    "target": {
      "section": "video",
      "reference": "<model-ref>",
      "path": "<daemon-internal-workspace-input-path>"
    }
  }
}
```

Read status and result:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/video/understanding/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o video-understanding.txt
```

Result route behavior matches audio transcription: queued/running jobs return
`409 result_pending`; failed, interrupted, or canceled jobs return a terminal
conflict; ready results return bytes with `Content-Type`,
`Content-Disposition`, `x-tentgent-next-cursor`, `x-tentgent-result-done`, and
`x-tentgent-chunks-read`.

## Image Generation Jobs

Canonical text-to-image generation uses a workflow job:

```http
POST /v1/images/generations/job
Content-Type: application/json
```

Request body:

```json
{
  "model_ref": "<image-generation-model-ref>",
  "adapter_ref": "<optional-image-lora-adapter-ref>",
  "lora_scale": 0.8,
  "prompt": "A small ceramic teapot on a wooden table",
  "negative_prompt": "optional negative prompt",
  "output_format": "png",
  "output_filename": "teapot.png",
  "width": 512,
  "height": 512,
  "steps": 20,
  "guidance_scale": 7.5,
  "seed": 42
}
```

Fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt for image generation. |
| `negative_prompt` | no | string | Optional negative prompt. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

The daemon uses the same image-generation runtime selection as the CLI:
Diffusers models use the Diffusers backend, and MLX `mlx-diffusion` models use
MFLUX on Apple Silicon macOS. Set `TENTGENT_IMAGE_GENERATION_DEVICE=cpu`,
`mps`, or `cuda` before daemon startup to force a Diffusers device for
image-generation jobs. Set
`TENTGENT_IMAGE_GENERATION_TORCH_DTYPE=float32` or `float16` only for
model/runtime compatibility debugging.
When `adapter_ref` is present, the daemon validates the adapter against the
selected image-generation model and backend before runtime execution.

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List generated files after completion:

```http
GET /v1/images/generations/job/{job_id}/files
```

Example response:

```json
{
  "files": [
    {
      "file_id": "teapot.png",
      "filename": "teapot.png",
      "media_type": "image/png",
      "format": "png",
      "total_bytes": 12345
    }
  ]
}
```

Download one generated file:

```http
GET /v1/images/generations/job/{job_id}/files/{file_id}
```

File download returns the image bytes with `Content-Type`,
`Content-Disposition`, `x-tentgent-job-id`, and `x-tentgent-file-id` headers.
Before completion, file routes return HTTP `409` with `result_pending`.
Failed, interrupted, or canceled jobs return clear terminal conflict errors.

## Image Transform Jobs

Canonical image-to-image transform uses a workflow job:

```http
POST /v1/images/transforms/job
Content-Type: multipart/form-data
```

The request uploads image bytes, not a client-local path. In curl,
`-F image=@/absolute/path/input.png` is client-side shorthand for reading that
file and sending bytes to the daemon.

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | PNG, JPEG, or WebP input image. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt describing the transform. |
| `negative_prompt` | no | string | Optional negative prompt. |
| `strength` | no | number | Defaults to `0.6`. Must be 0..1. `0.0` preserves input most; `1.0` regenerates most. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

The daemon persists the uploaded image into the job workspace before the worker
starts. Diffusers image-to-image receives `strength` directly. MFLUX-backed
`mlx-diffusion` models receive the equivalent image-influence value through
MFLUX after Tentgent maps the public Diffusers-style strength.

Response shape matches text-to-image jobs:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List transformed files after completion:

```http
GET /v1/images/transforms/job/{job_id}/files
```

Download one transformed file:

```http
GET /v1/images/transforms/job/{job_id}/files/{file_id}
```

Before completion, file routes return HTTP `409` with `result_pending`.
Terminal failures mirror the text-to-image job behavior.

## Image Inpaint Jobs

Canonical masked inpainting uses a workflow job:

```http
POST /v1/images/inpaint/job
Content-Type: multipart/form-data
```

The request uploads one base image and one mask image as bytes, not
client-local paths. Mask semantics are `white = repaint` and `black = keep`.
Tentgent normalizes the mask to binary grayscale before runtime execution.

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | PNG, JPEG, or WebP base image. |
| `mask` | yes | file bytes | PNG, JPEG, or WebP mask image. White pixels repaint; black pixels keep. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt describing the repaint. |
| `negative_prompt` | no | string | Optional negative prompt when the selected backend supports it. |
| `strength` | no | number | Defaults to `1.0`. Must be 0..1. `0.0` preserves the masked area most; `1.0` repaints most. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

Validation happens before model loading where practical:

- `image` and `mask` are both required and must be non-empty.
- Both file parts must be PNG, JPEG, or WebP by content type or file name.
- The Python runtime decodes both files with Pillow and requires matching
  decoded dimensions before resizing both to the requested output size.
- Diffusers inpainting receives `strength` directly.
- MFLUX inpainting requires a Flux Fill-compatible MLX model and maps
  Tentgent strength to the MFLUX image-influence parameter.

Response shape matches other image generation jobs:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List inpainted files after completion:

```http
GET /v1/images/inpaint/job/{job_id}/files
```

Download one inpainted file:

```http
GET /v1/images/inpaint/job/{job_id}/files/{file_id}
```

Before completion, file routes return HTTP `409` with `result_pending`.
Terminal failures mirror the text-to-image job behavior.

## Image Control Jobs

Canonical controlled image generation uses a workflow job:

```http
POST /v1/images/control/job
Content-Type: multipart/form-data
```

The request uploads one control image as bytes, not a client-local path. The
control image is paired with a managed ControlNet-style adapter referenced by
`control_ref`. This is separate from the optional image LoRA `adapter_ref`.

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `control_image` | yes | file bytes | PNG, JPEG, or WebP control image. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `control_ref` | yes | string | Managed ControlNet-style adapter ref or unique short-ref prefix. |
| `control_kind` | no | string | Defaults to `canny`; M6O supports `canny`. |
| `control_strength` | no | number | Defaults to `1.0`. Must be 0..2. Maps to Diffusers ControlNet conditioning scale. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt for the generated image. |
| `negative_prompt` | no | string | Optional negative prompt when the selected backend supports it. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

M6O expects the uploaded image to already be the control representation for the
selected `control_kind`; the daemon does not auto-run canny/depth/pose
preprocessors. Diffusers ControlNet is the first supported backend path. MLX
diffusion control returns an unsupported-backend error until a stable local
ControlNet-capable runtime is integrated.

For tiny ControlNet smoke fixtures, pass explicit small dimensions such as
`width=64`, `height=64`, and `steps=2`. The default `512x512` and `20` steps are
intended for normal image jobs and may be slow or exceed backend memory limits
on PyTorch MPS.

Response shape matches other image generation jobs:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List controlled generation files after completion:

```http
GET /v1/images/control/job/{job_id}/files
```

Download one controlled generation file:

```http
GET /v1/images/control/job/{job_id}/files/{file_id}
```

Before completion, file routes return HTTP `409` with `result_pending`.
Terminal failures mirror the text-to-image job behavior.

## Embeddings

```http
POST /v1/embeddings
Content-Type: application/json
```

```json
{
  "model_ref": "<embedding-model-ref>",
  "input": ["first text", "second text"]
}
```

`input` may be one string or an array of strings. The model must have
`embedding` capability metadata.

## Rerank

```http
POST /v1/rerank
Content-Type: application/json
```

```json
{
  "model_ref": "<rerank-model-ref>",
  "query": "refund policy",
  "documents": ["first candidate", "second candidate"],
  "top_n": 1
}
```

`documents` must be a non-empty string array. `top_n` is optional. The model
must have `rerank` capability metadata.

## Audio Transcription Jobs

Canonical audio transcription uses a workflow job:

```http
POST /v1/audio/transcriptions/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `file` | yes | file bytes | Audio bytes. The daemon does not receive or trust the client's local path. |
| `model_ref` | yes | text | Local `audio-transcription` model ref or unique alias. |
| `output_format` | no | text | `text`, `json`, `vtt`, or `srt`; defaults to `text`. |
| `language` | no | text | Use with multilingual checkpoints. Omit for English-only checkpoints. |
| `timestamps` | no | boolean text | `true`, `false`, `1`, `0`, `yes`, `no`, `on`, or `off`. |
| `output_filename` | no | text | File name only, not a path. |

`file` must appear exactly once. Audio transcription treats one request as one
logical audio input and one job. Send multiple audio files as multiple jobs, or
merge them before upload when a single transcript over a combined recording is
intended.

`vtt` and `srt` are subtitle formats. They require segment-level timestamps
from the selected backend; if the runtime cannot produce segment timings, the
job fails instead of writing untimed subtitles.

`curl` example:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<audio-transcription-model-ref> \
  -F output_format=text \
  -F file=@/absolute/path/audio.mp3
```

In client code, `file` can be any byte array placed into the multipart file
part. `file=@/absolute/path/audio.mp3` is only curl shorthand for "read this
local file and send its bytes"; it is not a path-based API contract. The daemon
stores those received bytes in the job workspace and then passes the internal
workspace file path to the runtime worker.

Only one `file` part is accepted per request. Send multiple recordings as
multiple jobs, or merge them client-side when one combined transcript is the
desired output.

The upload body is transport-stream friendly: clients may stream the multipart
request body, and the daemon writes the file part to disk instead of treating
the client's local path as input. This is an I/O and memory boundary, not a
promise that the selected model performs realtime or partial-file inference.
The daemon-wide media upload cap applies to the `file` file part.

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "audio_transcription",
    "status": "queued",
    "target": {
      "section": "audio",
      "reference": "<model-ref>",
      "path": "<daemon-internal-workspace-input-path>"
    }
  }
}
```

Read status and result:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o transcript.txt
```

Result route behavior:

| State | HTTP | Error code or response |
| --- | --- | --- |
| Job queued/running/intake | `409` | `result_pending` |
| Job failed | `409` | `job_failed` |
| Job interrupted | `409` | `job_interrupted` |
| Job canceled | `409` | `job_canceled` |
| Job succeeded but artifact missing | `404` | `result_not_found` |
| Result ready | `200` | Transcript bytes with `Content-Type`, `Content-Disposition`, `x-tentgent-next-cursor`, `x-tentgent-result-done`, and `x-tentgent-chunks-read`. |

Result reads are also transport-bounded: clients can read from `cursor` in
batches instead of requiring one full result read. Future large artifact routes
may stream response bodies or support range reads under workflow-owned routes;
they should not expose generic workspace or chunk internals.

Compatibility route:

```http
POST /v1/audio/transcriptions/jobs
GET  /v1/audio/transcriptions/jobs/{job_id}/result
```

The plural route is an undocumented alpha/debug compatibility path for trusted
local JSON path input. New clients should use the singular multipart route.

## Audio Speech

Canonical audio speech uses a workflow job:

```http
POST /v1/audio/speech/job
Content-Type: application/json
```

Request body:

```json
{
  "model_ref": "<audio-speech-model-ref>",
  "text": "Hello from Tentgent.",
  "output_format": "wav",
  "output_filename": "speech.wav",
  "language": "en",
  "voice": "default"
}
```

Fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `model_ref` | yes | string | Local `audio-speech` model ref or unique alias. |
| `text` | yes | string | Non-empty UTF-8 text. The default limit is 64 KiB. |
| `output_format` | no | string | `wav` or `wave`; defaults to `wav`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `speech.wav`. |
| `language` | no | string | Model-aware language hint. Unsupported values fail clearly. |
| `voice` | no | string | Model-aware voice or speaker hint. Unsupported values fail clearly. |

The route rejects unknown fields. `TENTGENT_AUDIO_SPEECH_MAX_TEXT_BYTES`
controls the maximum text byte length before daemon startup and defaults to
65536 bytes. M6P writes WAV only; `mp3`, realtime speech streaming,
speech-to-speech, SSML, and voice cloning are out of scope.

`curl` example:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/speech/job \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<audio-speech-model-ref>",
    "text": "Hello from Tentgent.",
    "output_format": "wav",
    "output_filename": "speech.wav"
  }'
```

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "audio_speech",
    "status": "queued",
    "target": {
      "section": "audio",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

Read status and result:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/audio/speech/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o speech.wav
```

Result route behavior:

| State | HTTP | Error code or response |
| --- | --- | --- |
| Job queued/running/intake | `409` | `result_pending` |
| Job failed | `409` | `job_failed` |
| Job interrupted | `409` | `job_interrupted` |
| Job canceled | `409` | `job_canceled` |
| Job succeeded but artifact missing | `404` | `result_not_found` |
| Result ready | `200` | WAV bytes with `Content-Type`, `Content-Disposition`, `x-tentgent-next-cursor`, `x-tentgent-result-done`, and `x-tentgent-chunks-read`. |

Result reads are cursor-based like audio transcription. They are an artifact
download boundary, not realtime model streaming.

## Jobs

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/jobs` | List daemon-managed jobs. |
| `GET` | `/v1/jobs/{job_id}` | Inspect one job. |
| `POST` | `/v1/jobs/{job_id}/cancel` | Cancel an active job when supported. |
| `DELETE` | `/v1/jobs/{job_id}` | Delete a terminal job record and workspace. Active jobs return conflict. |

Jobs are used for detached model/adapter/dataset operations and media
workflows. The daemon manages job workspaces; public APIs do not expose
workspace chunks or spool routes.

Cancellation updates the durable job state to terminal `canceled` for active
jobs and asks the daemon in-flight handle to abort. Already-started blocking
runtime work may continue outside the durable job state, so cancellation is a
best-effort worker interruption rather than a hard process-kill guarantee.
Terminal jobs are no longer cancellable.

Deleting a terminal job removes both the durable job record and the job
workspace when that workspace exists. Active jobs return `409 job_active`.
Daemon shutdown marks active daemon jobs `interrupted` and runs one
retention-aware workspace sweep; fresh interrupted or just-completed
workspaces are retained for inspection and result/recovery behavior.

## Models

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/models` | None. |
| `GET` | `/v1/models/{reference}` | None. |
| `DELETE` | `/v1/models/{reference}` | None. |
| `POST` | `/v1/models/{reference}/capabilities` | `{"set":["chat","vision-chat"]}` or `{"add":["vision-chat"],"remove":["chat"]}`. |
| `GET` | `/v1/models/{reference}/capabilities/proofs` | None. |
| `DELETE` | `/v1/models/{reference}/capabilities/proofs/{capability}` | None. |
| `POST` | `/v1/models/{reference}/capabilities/verify` | `{"capability":"chat\|embedding\|rerank\|audio-transcription\|audio-speech\|vision-chat\|video-understanding\|image-generation"}`. |
| `PATCH` | `/v1/models/{reference}` | Legacy compatibility alias for replacing the capability set with one `{"capability":"..."}` value. |
| `POST` | `/v1/models/import` | `{"path":"/absolute/model-dir","capability":"optional-capability"}` |
| `POST` | `/v1/models/pull` | `{"repo_id":"org/model","revision":"optional","capability":"optional-capability"}` |
| `POST` | `/v1/models/import/jobs` | Same as `/v1/models/import`, returns a job. |
| `POST` | `/v1/models/pull/jobs` | Same as `/v1/models/pull`, returns a job. |

Capability mutations canonicalize and de-duplicate values, set
`model_capability_source` to `manual-update`, and reject requests that would
leave a model with no capabilities.

Capability proofs are latest local records keyed by model and capability.
Manual `verify` is a metadata-level probe in this slice; local model-bound
server starts also write `server-start` proofs after launch success or failure.
Deleting a capability proof path clears all local proof records for that model
capability, including tuple-aware backend/runtime-profile records and the
legacy latest-proof file, without changing model content or capability
metadata. The delete response includes `proof_clear.capability` and
`proof_clear.removed_proof_count`.

## Adapters

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/adapters` | None. |
| `GET` | `/v1/adapters/{reference}` | None. |
| `DELETE` | `/v1/adapters/{reference}` | None. |
| `POST` | `/v1/adapters/import` | `{"path":"/absolute/adapter-dir","base_model_ref":"optional-base-model","target_capability":"optional-capability","adapter_type":"optional-type","adapter_format":"optional-format","backend_support":["optional-backend"],"control_kind":"optional-kind","weight_file":"optional-file","trigger_words":["optional-token"],"recommended_scale":0.8}` |
| `POST` | `/v1/adapters/pull` | `{"repo_id":"org/adapter","revision":"optional","base_model_ref":"optional-base-model","target_capability":"optional-capability","adapter_type":"optional-type","adapter_format":"optional-format","backend_support":["optional-backend"],"control_kind":"optional-kind","weight_file":"optional-file","trigger_words":["optional-token"],"recommended_scale":0.8}` |
| `POST` | `/v1/adapters/import/jobs` | Same as `/v1/adapters/import`, returns a job. |
| `POST` | `/v1/adapters/pull/jobs` | Same as `/v1/adapters/pull`, returns a job. |
| `POST` | `/v1/adapters/{reference}/bind` | `{"base_model_ref":"<base-model-ref>"}` |

Adapter import and pull metadata fields are optional. For image-generation LoRA
adapters, set `target_capability` to `image-generation`, use `adapter_format`
`diffusers-lora` or `mlx-diffusion-lora`, and include the corresponding
`backend_support` value `diffusers` or `mlx-diffusion`. `weight_file` is
required when the adapter source contains multiple candidate `.safetensors`
files. The `/jobs` variants accept the same metadata fields and return a daemon
job immediately.

For ControlNet-style image control adapters, set `target_capability` to
`image-generation`, `adapter_type` to `controlnet`, `adapter_format` to
`diffusers-controlnet`, `backend_support` to `["diffusers"]`, and
`control_kind` to a supported kind such as `canny`.

## Datasets

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/datasets` | None. |
| `GET` | `/v1/datasets/{reference}` | None. |
| `DELETE` | `/v1/datasets/{reference}` | None. |
| `POST` | `/v1/datasets/import` | `{"path":"/absolute/dataset-path"}` |
| `POST` | `/v1/datasets/import/jobs` | Same as `/v1/datasets/import`, returns a job. |
| `POST` | `/v1/datasets/validate` | `{"path":"optional-path","dataset_ref":"optional-ref"}` |
| `POST` | `/v1/datasets/template` | `{"task":"optional-task","language":"optional-language"}` |
| `POST` | `/v1/datasets/{reference}/export` | `{"output_path":"/absolute/output-path"}` |
| `POST` | `/v1/datasets/{reference}/diff` | `{"right_dataset_ref":"optional-ref","right_path":"optional-path"}` |
| `POST` | `/v1/datasets/synth/jobs` | Provider-backed dataset synthesis job through OpenAI, Anthropic, or Gemini. |
| `POST` | `/v1/datasets/eval/jobs` | Provider-backed dataset evaluation job through OpenAI, Anthropic, or Gemini. |

## LoRA Training

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/train/lora/plans` | None. |
| `POST` | `/v1/train/lora/plans` | `{"model_ref":"...","dataset_ref":"...","name":"optional","backend":"optional","overrides":{...}}` |
| `POST` | `/v1/train/lora/plans/preview` | Same as create, but does not persist. |
| `GET` | `/v1/train/lora/plans/{reference}` | None. |
| `DELETE` | `/v1/train/lora/plans/{reference}` | None. |
| `GET` | `/v1/train/lora/plans/{reference}/runs` | None. |
| `POST` | `/v1/train/lora/plans/{reference}/runs` | Starts a training run job. |
| `GET` | `/v1/train/lora/runs` | None. |
| `GET` | `/v1/train/lora/runs/{reference}` | None. |
| `GET` | `/v1/train/lora/runs/{reference}/metrics?tail=100` | Metrics tail. |
| `GET` | `/v1/train/lora/runs/{reference}/logs?tail_bytes=8192` | Log tail metadata and content. |
| `GET` | `/v1/train/lora/runs/{reference}/logs/raw?tail_bytes=8192` | Raw log tail. |

`overrides` may include `max_seq_length`, `mask_prompt`, `rank`,
`learning_rate`, `batch_size`, `gradient_accumulation_steps`, `max_steps`,
`seed`, `mlx_num_layers`, `mlx_grad_checkpoint`, `peft_load_in_4bit`, and
`peft_load_in_8bit`.

## Managed Servers

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/servers` | None. |
| `POST` | `/v1/servers` | `{"runtime_ref":"<model-or-cloud-ref>","capability":"optional chat\|embedding\|rerank\|audio-transcription\|audio-speech\|vision-chat\|video-understanding\|image-generation","host":"optional","port":8780,"lazy_load":true,"idle_seconds":60}` |
| `GET` | `/v1/servers/{reference}` | None. |
| `DELETE` | `/v1/servers/{reference}` | Removes a stopped server spec. |
| `POST` | `/v1/servers/{reference}/start` | `{"wait_ready":true,"timeout_seconds":30}` |
| `POST` | `/v1/servers/{reference}/stop` | None. |
| `GET` | `/v1/servers/{reference}/health` | Probe server process health. |
| `GET` | `/v1/servers/{reference}/logs` | Server log metadata. |
| `GET` | `/v1/servers/{reference}/logs/stdout?tail_bytes=8192` | Server stdout tail. |
| `GET` | `/v1/servers/{reference}/logs/stderr?tail_bytes=8192` | Server stderr tail. |

Direct model-server ports are separate from the daemon port. A server exposes
only the endpoint family selected by its `capability`, such as `/v1/chat`,
`/v1/embeddings`, `/v1/rerank`, audio, vision, video, or image routes.
Omitting `port` creates an auto-port server spec that starts scanning at `8780`
on every launch. Explicit `port` values are fixed. Server responses expose
`requested_port`, `port_auto`, and the running process `bound_port`; the top-level
`port` is the effective port clients should call.
Unsupported endpoint families on that direct server should return `404` or an
endpoint-specific error.

## Sessions

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/sessions` | None. |
| `POST` | `/v1/sessions` | `{"title":"optional","default_server_ref":"optional","adapter_ref":"optional","tags":[],"messages":[]}` |
| `GET` | `/v1/sessions/{reference}` | None. |
| `PATCH` | `/v1/sessions/{reference}` | `{"title":"new-or-null","default_server_ref":"new-or-null","adapter_ref":"new-or-null","tags":["..."]}` |
| `DELETE` | `/v1/sessions/{reference}` | None. |
| `GET` | `/v1/sessions/{reference}/messages?tail=100` | Session transcript tail. |
| `POST` | `/v1/sessions/{reference}/messages` | `{"messages":[{"role":"user","content":"...","server_ref":"optional","adapter_ref":"optional","metadata":{}}],"compaction_server_ref":"optional"}` |
| `POST` | `/v1/sessions/{reference}/compact` | `{"server_ref":"optional","keep_recent_messages":49,"instructions":"optional"}` |

Session messages are text records. Multimodal chat transcript content is not
implemented yet.
