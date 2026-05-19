# M6C Through M6H Media Runtime Roadmap

This document expands the remaining M6 multimodal work after
[M6A](./m6a-multimodal-contracts.md) and
[M6B](./m6b-kernel-job-workspace-foundation.md).

Status: proposed.

## Direction

- M6B is the shared kernel job workspace foundation only. It does not execute
  media models.
- Native workflow endpoints should use parsed request fields, explicit model
  capabilities, job progress, and kernel job workspaces.
- CLI media workflows should stay simple: input file or prompt in, output file
  out. Foreground CLI commands are not durable jobs. Job IDs are used only for
  detached/background work and advanced/debug handles.
- Opaque byte streaming and realtime duplex transports are separate later work,
  not the default model-serving path.

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
- `output_format`
- optional `output_filename`
- workflow-specific options such as language, timestamps, seed, dimensions, or
  voice

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

Daemon API:

```text
POST /v1/audio/transcriptions/jobs
POST /v1/audio/transcriptions/upload/jobs
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

Upload request uses `multipart/form-data`:

```text
file=@audio.wav
model_ref=<audio-transcription-model-ref>
language=en
output_format=text
timestamps=false
```

Output formats:

```text
text -> transcript.txt, text/plain
json -> transcript.json, application/json
vtt  -> transcript.vtt, text/vtt
srt  -> transcript.srt, application/x-subrip
```

Initial runtime target:

- Whisper tiny-class models through the Python local runtime.
- Non-realtime batch transcription first.
- Chunk/segment processing may be internal to the worker; M6B workspace chunks
  are storage chunks, not codec-aware audio segments.

Review target:

- A small local audio model can turn a short audio file into transcript bytes
  through daemon jobs and
  `/v1/audio/transcriptions/jobs/{job_id}/result`.

## M6D: CLI Media Workflow Wrapper

Goal: make the first media workflow ergonomic from the `tentgent` CLI and add
low-level job control helpers only where useful.

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

Review target:

- A user can run one command from local audio file to local transcript file
  without manually managing job IDs.

## M6E: Audio Speech

Goal: add text-to-speech as a second audio workflow after transcription proves
the workspace/result path.

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

CLI:

```bash
tentgent speak \
  --model-ref <audio-speech-model-ref> \
  --text "Hello from Tentgent." \
  --output speech.wav \
  --format wav
```

Review target:

- A text prompt can produce a local audio result file through daemon jobs.

## M6F: Image Generation

Goal: add image artifact generation after binary result handling is proven by
audio speech.

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

## M6G: Vision Chat

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

The synchronous route is allowed for small images. Job routes are used for
larger images or slower model paths.

Path job request:

```json
{
  "model_ref": "<vision-chat-model-ref>",
  "path": "/absolute/path/image.png",
  "prompt": "Describe this image.",
  "output_format": "text"
}
```

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

## M6H: Video And Realtime Streaming Decision

Goal: decide whether Tentgent needs native video understanding, realtime audio
or video, an opaque runtime stream proxy, or all three.

Potential capabilities:

```text
video-understanding
```

Do not add this capability until payload and result semantics are approved.

Candidate native API:

```text
POST /v1/video/understanding/jobs
POST /v1/video/understanding/upload/jobs
```

Candidate CLI:

```bash
tentgent video-understand /path/to/video.mp4 \
  --model-ref <video-understanding-model-ref> \
  --message "Summarize the main events." \
  --output summary.md \
  --format md
```

Realtime evaluation:

- Use WebSocket first for local daemon duplex control and byte chunks.
- Consider WebRTC only when browser-native low-latency media becomes a product
  requirement.
- Keep opaque raw stream proxy routes separate from native workflow routes, for
  example under a future `/v1/runtime/streams` namespace.

Review target:

- The project has a clear go/no-go decision for realtime and video before
  adding transport infrastructure.

## Release Ordering

Recommended order:

1. Finish M6B kernel job workspace refactor and cleanup gaps that block
   workflow workers.
2. Implement M6C daemon audio transcription.
3. Add M6D CLI wrapper for transcription and generic job control helpers.
4. Add M6E audio speech.
5. Add M6F image generation.
6. Add M6G vision chat.
7. Decide M6H video/realtime/opaque stream proxy.

Apple signing can still run in parallel before beta. It does not need to wait
for all M6 runtime workflows.
