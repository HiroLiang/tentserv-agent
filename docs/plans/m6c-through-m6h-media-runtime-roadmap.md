# M6C And Later Media Runtime Roadmap

This document expands the remaining M6 multimodal work after
[M6A](./m6a-multimodal-contracts.md) and
[M6B](./m6b-kernel-job-workspace-foundation.md).

Status: M6C path-job MVP implemented and smoke-tested; M6D and later slices
proposed.

## Direction

- M6B is the shared kernel job workspace foundation only. It does not execute
  media models.
- Native workflow endpoints should use parsed request fields, explicit model
  capabilities, job progress, and kernel job workspaces.
- External workflow APIs expose product capabilities, not workspace internals.
  Chunk files, terminal markers, partial buffers, and garbage collection are
  kernel/daemon implementation details.
- Upload chunks are transport/storage units. Model chunks are decoded media
  windows or sampled visual clips. Do not treat arbitrary uploaded bytes as
  model-ready context.
- Batch media runtimes should wait for a complete logical input file before
  execution unless a runtime explicitly supports partial streaming semantics.
- CLI media workflows should stay simple: input file or prompt in, output file
  out. Foreground CLI commands are not durable jobs. Job IDs are used only for
  detached/background work and advanced/debug handles.
- Realtime duplex transports and opaque backend proxy routes are separate later
  work, not the default workflow path.

## Shared API Rules

All async media workflow routes return a normal job response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "audio_transcription",
    "status": "queued",
    "result_summary": null
  }
}
```

Result bytes are read through:

```text
GET /v1/<workflow>/jobs/{job_id}/result?cursor=0&max_chunks=32
```

Feature-specific result routes may use raw bytes plus cursor headers. Workflow
routes choose the result media type and file extension by `output_format`.

Common request fields:

- `model_ref`
- `path` for daemon-host local files, when the workflow consumes a file
- `file` for multipart upload routes, when the caller sends media bytes
- `output_format`
- optional `output_filename`
- workflow-specific options such as language, timestamps, seed, dimensions, or
  voice

Common media intake rules:

- Path requests require absolute daemon-host paths.
- Upload requests receive a complete logical file from the user. The daemon may
  internally persist chunks while the request body is arriving, but workers see
  a declared input file, not public chunk IDs.
- Batch workers start after the logical input file is complete. Streaming
  workers may start earlier only when their runtime port advertises partial
  input support.
- Job workspaces retain terminal, interrupted, canceled, and failed
  inputs/results for the configured buffer window before garbage collection.
- File-type validation belongs to workflow adapters. Users should not be asked
  to pre-decode audio, images, or video unless a specific advanced backend
  documents that requirement.

Model input normalization:

- Audio workflows accept audio/video container files at the API boundary.
  Runtime adapters may use `ffmpeg`, `torchaudio`, or model pipelines to decode
  into waveform/features before inference.
- Image vision workflows use typed content parts: text parts and image parts
  are distinct. Images are decoded into pixel tensors or visual tokens inside
  the runtime adapter.
- Video understanding workflows accept video files, then sample frames or clips
  with timestamps before inference. They must avoid loading whole videos into
  memory.
- Generation workflows write artifacts. Image generation writes image files;
  speech writes audio files; video generation writes encoded video files after
  frames are produced.

Common CLI behavior:

- Text-like results may print to stdout when `--output` is omitted.
- Binary results require `--output` unless a command-specific default path is
  accepted.
- Default output file names derive from the input stem or prompt slug.
- Foreground CLI commands should run as one-shot commands and should not appear
  in `tentgent jobs ls`.
- CLI commands may create daemon jobs only for explicit detached/background
  mode, then poll progress, read result chunks, and write the requested output
  path without exposing workspace/chunk operations.

## M6C: Audio Transcription Daemon MVP

Goal: implement the first native media runtime workflow on top of the kernel
job workspace foundation.

Capability:

```text
audio-transcription
```

Implemented daemon API:

```text
POST /v1/audio/transcriptions/jobs
GET /v1/audio/transcriptions/jobs/{job_id}/result?cursor=0&max_chunks=32
```

Path request:

```json
{
  "model_ref": "<audio-transcription-model-ref>",
  "path": "/absolute/path/audio.wav",
  "language": "en",
  "output_format": "text",
  "timestamps": false
}
```

Output formats:

```text
text -> transcript.txt, text/plain
json -> transcript.json, application/json
vtt  -> transcript.vtt, text/vtt
srt  -> transcript.srt, application/x-subrip
```

Completed scope:

- Whisper tiny-class models through the Python local runtime.
- Non-realtime batch transcription first.
- Daemon-local absolute path input.
- Workflow-owned result chunks in the kernel job workspace.
- `ffmpeg` doctor warning with OS-specific install guidance.
- English-only Whisper fallback retries without `language` when the backend
  rejects language hints.

Completion evidence:

- Kernel audio domain, ports, use case, runtime adapter, and tests exist.
- Daemon exposes audio transcription job creation and result retrieval routes.
- Python runtime exposes `tentgent-audio-transcribe` and local `transformers`
  ASR backend support.
- `openai/whisper-tiny.en` was smoke-tested against `test-data/test_audio.mp3`
  through daemon jobs after installing `ffmpeg`.

Deferred from M6C:

- Multipart upload.
- Foreground `tentgent transcribe` CLI.
- Bounded-memory audio decoding/windowing beyond the current Transformers ASR
  pipeline behavior.

## M6D: Media File Intake And Upload Foundation

Goal: add upload-capable workflow intake without exposing low-level workspace
chunk APIs.

Daemon API pattern:

```text
POST /v1/<workflow>/jobs
POST /v1/<workflow>/upload/jobs
```

Rules:

- `POST /jobs` accepts daemon-host absolute path requests.
- `POST /upload/jobs` accepts multipart file upload plus workflow fields.
- The daemon may write upload bytes into ordered workspace chunks while the
  HTTP request body is arriving, but it assembles or declares one logical input
  file before batch workers run.
- Public job APIs expose job status, cancel/delete, and workflow result reads.
  They do not expose chunk write/read/list endpoints.
- Slow uploads keep the job in an intake/queued state. Missing bytes, timeout,
  or client disconnect marks the job interrupted or failed and leaves the
  workspace for the retention buffer.
- GC must retain completed, interrupted, canceled, and failed workspaces for
  the configured buffer window before cleanup.

Review target:

- A workflow can accept a large uploaded file, persist it through the job
  workspace, run after upload completion, and return the same result behavior
  as a path request.

## M6E: Audio Transcription CLI And Large-File Hardening

Goal: make the first media workflow ergonomic from the `tentgent` CLI and add
large-file safety around audio decoding.

User command:

```bash
tentgent transcribe /path/to/audio.wav \
  --model-ref <audio-transcription-model-ref> \
  --output transcript.txt \
  --format text
```

Optional advanced helpers:

```bash
tentgent jobs inspect <job-id>
tentgent jobs cancel <job-id>
tentgent jobs delete <job-id>
```

Rules:

- `transcribe` foreground mode writes the requested output without registering
  itself as a durable job.
- Foreground mode should use a direct one-shot runtime path or an explicitly
  foreground daemon route. It must not create a hidden durable job.
- `transcribe --detach` may create a daemon job, return a `job_id`, and rely on
  job inspect/cancel/delete helpers.
- `--format` maps to daemon `output_format`.
- If `--output` is omitted for `text` or `json`, print to stdout.
- For `vtt` and `srt`, prefer requiring `--output` unless stdout behavior is
  explicitly useful.
- Users provide an audio file. They should not need to pre-decode to PCM.
- For batch ASR, execution starts after the file is complete.
- Runtime adapters should prefer bounded-memory decode/window paths for large
  files instead of relying on whole-file decoded arrays.
- Multipart upload support comes from M6D; this slice wires it into the audio
  workflow.

Review target:

- A user can run one command from local audio file to local transcript file
  without manually managing job IDs.

## M6F: Vision Chat Image Input

Goal: add image plus text to text output after image input handling and
content-part DTOs are explicit.

Capability:

```text
vision-chat
```

Daemon API:

```text
POST /v1/vision/chat
POST /v1/vision/chat/jobs
POST /v1/vision/chat/upload/jobs
```

Typed content request:

```json
{
  "model_ref": "<vision-chat-model-ref>",
  "messages": [
    {
      "role": "user",
      "content": [
        { "type": "image", "path": "/absolute/path/image.png" },
        { "type": "text", "text": "Describe this image." }
      ]
    }
  ],
  "output_format": "text"
}
```

Rules:

- Text and image inputs are separate typed content parts.
- Small synchronous image requests are allowed.
- Job/upload routes are used for larger images or slower model paths.
- Images should be complete before runtime execution. Large images may be
  resized or tiled by the runtime adapter.

Output formats:

```text
text -> vision-answer.txt, text/plain
json -> vision-answer.json, application/json
md   -> vision-answer.md, text/markdown
```

CLI:

```bash
tentgent vision-chat /path/to/image.png \
  --model-ref <vision-chat-model-ref> \
  --message "Describe this image." \
  --output answer.md \
  --format md
```

Review target:

- An image and prompt can produce a text answer without reusing the text-only
  chat DTO in an ambiguous way.

## M6G: Image Generation

Goal: add image artifact generation after binary result handling and prompt
generation DTOs are explicit.

Capability:

```text
image-generation
```

Daemon API:

```text
POST /v1/images/generations/jobs
```

Request:

```json
{
  "model_ref": "<image-generation-model-ref>",
  "prompt": "a small product icon on a white background",
  "negative_prompt": null,
  "width": 512,
  "height": 512,
  "seed": null,
  "output_format": "png"
}
```

Output formats:

```text
png -> image.png, image/png
jpg -> image.jpg, image/jpeg
```

CLI:

```bash
tentgent image-generate \
  --model-ref <image-generation-model-ref> \
  --prompt "a small product icon on a white background" \
  --output image.png \
  --format png
```

Reference-image and mask inputs should use job workspace inputs, but they can
be a later sub-slice after text-to-image works.

Review target:

- A prompt can produce a local image file through daemon jobs and result
  cursors.

## M6H: Audio Speech

Goal: add text-to-speech after audio input/output artifact rules are proven.

Capability:

```text
audio-speech
```

Daemon API:

```text
POST /v1/audio/speech/jobs
```

Request:

```json
{
  "model_ref": "<audio-speech-model-ref>",
  "input": "Hello from Tentgent.",
  "voice": null,
  "output_format": "wav",
  "sample_rate": null
}
```

Output formats:

```text
wav -> speech.wav, audio/wav
flac -> speech.flac, audio/flac
```

`mp3` should wait until an encoder dependency and licensing boundary are
explicitly approved.

Review target:

- A text prompt can produce a local audio result file through daemon jobs.

## M6I: Video Understanding

Goal: add file-based video understanding without loading whole videos into
memory.

Potential capability:

```text
video-understanding
```

Do not add this capability until payload and result semantics are approved.

Daemon API:

```text
POST /v1/video/understanding/jobs
POST /v1/video/understanding/upload/jobs
```

Request:

```json
{
  "model_ref": "<video-understanding-model-ref>",
  "path": "/absolute/path/video.mp4",
  "prompt": "Summarize the main events.",
  "fps": 1.0,
  "max_pixels": null,
  "output_format": "md"
}
```

Rules:

- Video request input is a complete logical video file, provided by path or
  upload.
- Internal storage may use upload chunks, but runtime adapters consume a file
  and sample frames/clips with timestamps.
- Video understanding is normally prompt-driven text output, not a generic
  single-purpose model route. Pipeline-specific tasks may later narrow this
  into captioning, detection, or classification workflows.
- Workers must stream decode/sample frames or clips and avoid whole-file memory
  loading.

Output formats:

```text
text -> video-answer.txt, text/plain
json -> video-answer.json, application/json
md   -> video-answer.md, text/markdown
```

Review target:

- A video file and prompt can produce a text/Markdown/JSON result through jobs
  using bounded frame sampling.

## M6J: Video Generation Artifacts

Goal: decide whether local video generation should be supported in the current
release line and, if yes, implement it as artifact-producing jobs.

Potential capability:

```text
video-generation
```

Do not add this capability until payload and result semantics are approved.

Rules:

- Video generation should be job-only.
- Inputs may be text prompts, image references, or future video conditioning
  files.
- Runtime adapters generally produce frames first and then encode/export a
  video artifact.
- Results should be playable files such as `mp4` or `webm`; raw frames are a
  debug/advanced artifact only.
- Encoding dependencies and hardware expectations must be documented before a
  user-facing command ships.

Review target:

- The project has a go/no-go decision for local video generation and a clear
  artifact contract before runtime implementation.

## M6K: Media Serving And Runtime Stream Proxy Decision

Goal: separate long-lived model serving from durable workflow jobs.

Server direction:

- `tentgent server` may launch a pinned model/capability and expose
  capability-native routes so callers can send files or prompts to a stable
  port without learning whether the backend is MLX, llama.cpp, Transformers, or
  another runtime.
- Direct serving should be normalized by capability, not raw backend internals
  by default. For example, an audio transcription server can expose an
  OpenAI-style multipart `/v1/audio/transcriptions` route while the daemon
  chooses the runtime adapter underneath.
- Direct serving is appropriate for warm models, low latency, small or bounded
  requests, and supported streaming outputs.
- Long-running generation, very large uploads, and resumable work should return
  durable jobs instead of holding one HTTP request forever.
- A truly opaque backend proxy may be added later under a separate namespace,
  but it must be explicitly marked advanced because it leaks backend-specific
  request and response shapes.

Realtime evaluation:

```text
WebSocket first for local duplex control.
WebRTC only when browser-native low-latency media is a product requirement.
```

Review target:

- The project chooses which media capabilities get long-lived server routes and
  which remain job-only.

## Release Ordering

Recommended order:

1. Finish M6B kernel job workspace refactor and cleanup gaps that block
   workflow workers.
2. Implement M6C daemon audio transcription.
3. Add M6D media file intake and upload foundation.
4. Add M6E audio transcription CLI and large-file hardening.
5. Add M6F vision chat image input.
6. Add M6G image generation.
7. Add M6H audio speech.
8. Add M6I video understanding.
9. Decide M6J video generation artifacts.
10. Decide M6K media serving and runtime stream proxy scope.

Apple signing can still run in parallel before beta. It does not need to wait
for all M6 runtime workflows.
