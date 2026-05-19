# M6C Audio Transcription Daemon MVP

This document records the M6C implementation only. M6D has its own execution
record, and M6E-and-later planning belongs in
[capability-first-release-roadmap.md](./capability-first-release-roadmap.md).

Status: implemented and smoke-tested.

## Context

- M6A introduced metadata-only multimodal capability names.
- M6B introduced kernel job workspace foundations.
- M6C is the first media runtime execution slice and proves audio
  transcription jobs before upload, CLI, vision, image generation, speech, or
  video slices.

## Goal

Implement the first native media runtime workflow on top of the kernel job
workspace foundation.

Capability:

```text
audio-transcription
```

## Implemented Daemon API

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

## Completed Scope

- Whisper tiny-class models through the Python local runtime.
- Non-realtime batch transcription first.
- Daemon-local absolute path input.
- Workflow-owned result chunks in the kernel job workspace.
- `ffmpeg` doctor warning with OS-specific install guidance.
- English-only Whisper fallback retries without `language` when the backend
  rejects language hints.

## Implementation Notes

- Kernel audio domain, ports, use case, runtime adapter, and tests exist.
- Daemon exposes audio transcription job creation and result retrieval routes.
- Python runtime exposes `tentgent-audio-transcribe` and local `transformers`
  ASR backend support.
- Result files are written into the job workspace and then read through the
  audio transcription result route.
- Job status and result metadata reuse the kernel job workspace foundation from
  M6B.

## Completion Evidence

- `openai/whisper-tiny.en` was smoke-tested against local MP3 audio through
  daemon jobs after installing `ffmpeg`.
- Kernel audio tests cover output format parsing, model resolver checks, and
  Python runtime entrypoint argument construction.
- Daemon REST tests cover path request validation and result chunk reads.

## Deferred From M6C

- Multipart file upload. Implemented later by M6D.
- Foreground `tentgent transcribe` CLI. Implemented later by M6E.
- Bounded-memory audio decoding/windowing beyond the current Transformers ASR
  pipeline behavior. M6E added first-layer CLI guardrails; deeper runtime
  windowing remains later work.
