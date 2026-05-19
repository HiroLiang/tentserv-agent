# M6D Audio Transcription File-Stream Job Input

Status: implemented.

Depends on:

- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- M6C audio transcription path-job MVP, committed as
  `d62aa94 Implement M6C audio transcription jobs`

## Goal

Expose the product-shaped audio transcription job API:

```text
POST /v1/audio/transcriptions/job
```

The endpoint accepts an audio file stream, stores the complete input in the
job workspace, starts daemon-managed audio transcription work after the file is
complete, and returns a `job_id`. Users can then inspect, cancel, delete, and
read the result through public job and workflow endpoints. Users never manage
workspace chunks, spool objects, or media-store references.

## Product Contract

Canonical workflow endpoints:

```text
POST /v1/audio/transcriptions/job
GET  /v1/audio/transcriptions/job/{job_id}/result?cursor=0&max_chunks=32
```

Generic job controls remain plural because they operate on the job registry:

```text
GET    /v1/jobs/{job_id}
POST   /v1/jobs/{job_id}/cancel
DELETE /v1/jobs/{job_id}
```

The create route accepts `multipart/form-data`:

```text
file=@audio.mp3
model_ref=<audio-transcription-model-ref>
output_format=text
language=en
timestamps=false
output_filename=transcript.txt
```

Field rules:

- `file` is required and must appear exactly once.
- `model_ref` is required.
- `output_format` defaults to `text` and supports `text`, `json`, `vtt`, and
  `srt`.
- `language`, `timestamps`, and `output_filename` are optional.
- Metadata fields must have bounded size.
- The uploaded file is stored as one logical input file. Internal chunking is
  allowed only as a daemon/kernel workspace detail.

Single-request semantics:

- The daemon creates an internal job record while receiving the request body.
- In normal HTTP multipart flow, the response with `job_id` is sent after the
  file stream is accepted, the logical input file is declared, and worker
  execution is queued or started.
- Returning a `job_id` before the upload body completes would require a
  two-phase upload session or duplex transport. That is out of scope for M6D
  because it would reintroduce user-visible upload/session management.

## Non-Goals

- Do not expose `/v1/spool/*`, `/v1/workspaces/*`, or chunk read/write APIs.
- Do not create a separate `/upload/jobs` route.
- Do not add realtime ASR or partial transcript streaming.
- Do not require users to pre-decode audio to PCM.
- Do not implement the foreground `tentgent transcribe` CLI in this slice.
- Do not solve bounded-memory ASR decoding for very large audio files; M6D
  should prepare the file-stream intake path, while M6E can harden decode and
  windowing.
- Do not implement audio direct server routes. Server direct serving remains a
  later media-server slice.

## Current Baseline

M6C already provides:

- `POST /v1/audio/transcriptions/jobs` with JSON path input.
- `GET /v1/audio/transcriptions/jobs/{job_id}/result`.
- Kernel audio transcription domain, ports, resolver, runtime client, and
  use case.
- Python `tentgent-audio-transcribe` runtime entrypoint.
- A Transformers ASR backend that accepts a local input path and writes a
  transcript file.
- Job workspace result chunks and result-file metadata.
- `result_pending` when no result chunks exist and the job is not terminal.

M6D should keep the M6C worker path and replace only how uploaded input reaches
the worker.

## Execution Plan

### 1. Route Shape

- Add the canonical singular routes to the Rust REST router:

```text
POST /v1/audio/transcriptions/job
GET  /v1/audio/transcriptions/job/{job_id}/result
```

- Move user docs, tests, and examples to the singular routes.
- Keep the current plural M6C routes only as undocumented alpha compatibility
  aliases if needed. If release compatibility is not required, remove the
  plural audio routes in this slice.
- Keep generic job registry routes under `/v1/jobs`.

### 2. Multipart Request Parsing

- Add multipart parsing to the audio REST handler.
- Prefer `axum::extract::Multipart`; enable the needed `axum` feature or add a
  narrow multipart parser dependency only if required.
- Parse and validate:
  - one required `file` part
  - required `model_ref`
  - optional `output_format`
  - optional `language`
  - optional `timestamps`
  - optional `output_filename`
- Reject unknown or duplicate critical fields with clear `bad_request` errors.
- Bound field sizes for metadata and reject empty `model_ref`.
- Preserve the original upload filename only as metadata and sanitize any name
  used on disk.

### 3. Workspace Input File Persistence

- Create the job and open its workspace before consuming the file stream.
- Store the uploaded bytes under the job workspace as a logical input file.
- Use internal chunk writes only if useful for crash-safe persistence. Do not
  expose chunk identifiers.
- Write to a temporary/partial path first, then atomically finalize/rename the
  file when the upload completes.
- Finalize the input stream manifest with:
  - state `done`
  - media type inferred from filename/content where practical
  - original filename
  - total bytes
  - chunk count if internal chunks are used
- If upload fails or the client disconnects, mark the job failed or
  interrupted, finalize the input manifest as failed where possible, and retain
  the workspace for the configured buffer window.

### 4. Job Lifecycle And Worker Start

- The daemon owns the job lifecycle.
- The user receives only a `job_id`.
- While receiving the file, the job should expose a clear stage such as
  `receiving audio input`.
- After upload completion, update the stage to `running audio transcription`.
- Start the existing M6C audio transcription worker only after the logical input
  file is complete.
- Reuse the current audio use case by passing the finalized workspace input
  path to `AudioTranscriptionPreparationRequest`.
- Keep result chunk writing and result-file declaration behavior from M6C.

### 5. Result Endpoint Semantics

Update result handling so every pre-ready or terminal state is explicit:

- `queued`, `running`, or receiving input:
  - error code: `result_pending`
  - message: result is not ready yet
- `failed`:
  - error code: `job_failed`
  - message: job failed; inspect `/v1/jobs/{job_id}` for details
- `interrupted`:
  - error code: `job_interrupted`
  - message: job was interrupted before producing a result
- `canceled`:
  - error code: `job_canceled`
  - message: job was canceled before producing a result
- `succeeded` but no result artifact:
  - error code: `result_not_found`
- `succeeded` with result chunks:
  - return bytes, content type, filename, cursor headers, and done header

### 6. Path Input Compatibility

The product API is file-stream first. Path input is useful only for local
daemon debugging and trusted local tools.

Implementation options:

- Preferred release shape: canonical singular route accepts multipart only;
  path input is removed from public user docs.
- Temporary alpha shape: keep JSON path input as a compatibility branch on the
  same singular endpoint or the old plural endpoint, but mark it undocumented.

Do not present path input as the primary user-facing API after M6D.

### 7. User Documentation

Update user-facing docs to show the canonical file-stream command:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<audio-transcription-model-ref> \
  -F output_format=text \
  -F file=@/absolute/path/audio.mp3
```

Document that:

- The response is a job response.
- Result reads before completion return `result_pending`.
- Users should inspect `/v1/jobs/{job_id}` for progress and terminal errors.
- `ffmpeg` is required for common compressed audio inputs.
- Workspace chunks are internal implementation details.

### 8. Tests

Add or update daemon REST tests for:

- `POST /v1/audio/transcriptions/job` accepts multipart upload and returns a
  job response.
- Missing `file` returns a clear `bad_request`.
- Missing or blank `model_ref` returns a clear `bad_request`.
- Unsupported `output_format` returns a clear `bad_request`.
- Duplicate file or duplicate critical fields return a clear `bad_request`.
- Uploaded input is stored in the job workspace and passed to the existing
  audio runtime worker path.
- Result route returns `result_pending` before result chunks exist.
- Result route returns terminal-specific errors for failed/interrupted/canceled
  jobs without artifacts.
- Result route returns bytes, media type, filename, and cursor headers after
  success.
- Old plural routes are either removed from tests or tested only as
  undocumented compatibility aliases.

Add or update kernel job tests only if M6D needs new workspace helpers for
logical input files.

## Likely Files

Rust daemon:

- `src/tentgent-daemon/src/transport/rest/router.rs`
- `src/tentgent-daemon/src/handlers/rest/audio/mod.rs`
- `src/tentgent-daemon/src/transport/rest/tests.rs`

Kernel job workspace, only if new helpers are needed:

- `src/tentgent-kernel/src/features/job/domain.rs`
- `src/tentgent-kernel/src/features/job/ports.rs`
- `src/tentgent-kernel/src/features/job/infra/workspace.rs`
- `src/tentgent-kernel/src/features/job/tests.rs`

Docs:

- `docs/user/commands.md`
- `docs/user/model-fixtures.md`
- `docs/user/version.md`
- `docs/plans/capability-first-release-roadmap.md`

## Verification

Required local checks:

```bash
cargo fmt
cargo check --workspace
cargo test -p tentgent-kernel job
cargo test -p tentgent-kernel audio
cargo test -p tentgent-daemon audio_transcription
cargo test --workspace
uv run python -m unittest discover -s tests
```

Recommended smoke test when a local ASR model is available:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<audio-transcription-model-ref> \
  -F output_format=text \
  -F file=@test-data/test_audio.mp3
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o transcript.txt
```

Review target:

- A caller can send an audio file stream to
  `POST /v1/audio/transcriptions/job`, receive a `job_id`, inspect or cancel
  the job through `/v1/jobs/{job_id}`, and read transcript bytes only after the
  daemon-managed job succeeds.

## Completion Notes

- Added canonical singular daemon routes:
  `POST /v1/audio/transcriptions/job` and
  `GET /v1/audio/transcriptions/job/{job_id}/result`.
- The create route accepts multipart form data, validates bounded metadata
  fields, stores the uploaded file in the job workspace, finalizes input
  workspace metadata, then starts the existing M6C audio transcription worker.
- The old plural JSON path route remains as an undocumented alpha/debug
  compatibility route.
- Result reads now distinguish pending jobs, failed/interrupted/canceled jobs,
  missing terminal artifacts, and ready transcript bytes.
- User docs and model fixture docs now show the file-upload job API.
