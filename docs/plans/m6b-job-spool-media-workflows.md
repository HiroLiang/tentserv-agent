# M6B Job Spool For Media Workflows

This is the focused execution plan for the second M6 slice in the
[capability-first release roadmap](./capability-first-release-roadmap.md).

Status: planned.

## Goal

- Add a daemon-owned, job-scoped spool for large media inputs and outputs.
- Use `job_id` as the user-visible handle instead of creating a long-lived
  `media_ref` catalog.
- Support path input and upload input without permanently importing every media
  file into `TENTGENT_HOME`.
- Add cleanup, TTL, quota, and crash-recovery rules before model runtime work.
- Leave audio, image, video, and realtime model execution to later M6C slices.

## Why This Replaces A Media Store

Audio, image, and video files can be large and frequent when Tentgent is used as
a tool runtime. Treating every input as a managed store object would create
unwanted SSD pressure and extra user-visible objects to clean up.

M6B should treat large media as job-local working data:

```text
path input / upload stream
        ↓
job-scoped input spool
        ↓
workflow worker / model runtime
        ↓
job-scoped result spool
        ↓
stream response / result download
        ↓
TTL and GC cleanup
```

A durable media catalog can be added later as an explicit "promote result" or
"save artifact" feature if users need long-term reuse. It is not the default
large-file path.

## Scope

Implement a first job spool subsystem:

- daemon runtime spool domain and file-backed implementation
- chunked input writer for path and upload sources
- chunked result writer and cursor-based result reader
- job metadata integration so `/v1/jobs` can show spool summary and cleanup
  state
- cleanup sweeps for expired, terminal, interrupted, and orphaned spools
- daemon stop integration that interrupts active spool jobs and runs one safe
  cleanup sweep
- contract docs for spool layout and lifecycle

M6B should not execute models. It should provide the shared large-file
transport layer that M6C audio transcription can consume first.

## Spool Layout

Job spools live under the existing daemon runtime job area:

```text
TENTGENT_HOME/
└── runtime/
    └── jobs/
        └── <job_id>/
            ├── job.json or job.toml
            └── spool/
                ├── input/
                │   ├── 0000000000000000.part
                │   ├── 0000000000000000.chunk
                │   ├── 0000000000000001.chunk
                │   └── input.done.toml
                └── result/
                    ├── 0000000000000000.chunk
                    ├── 0000000000000001.chunk
                    └── result.done.toml
```

Rules:

- Use monotonically increasing zero-padded hexadecimal sequence numbers for
  sortable chunk filenames.
- Write chunks as `.part`, then atomically rename to `.chunk` after fsync/write
  completion.
- Write a terminal manifest such as `input.done.toml` or `result.done.toml`
  only after all chunks are complete.
- Do not use timestamps as ordering keys. Timestamps belong in metadata.
- Keep raw chunk paths daemon-internal; clients observe `job_id`, status,
  summary, and result cursors.

## Manifest Shape

Each completed input or result stream should have a manifest:

- `stream`: `input` or `result`
- `state`: `open`, `done`, or `failed`
- `chunk_count`
- `total_bytes`
- optional `sha256`
- optional `media_type`
- optional `original_filename`
- optional `started_at`, `finished_at`
- optional `error_summary`

The manifest is the integrity boundary. Workers should not treat a stream as
complete until the done manifest exists. A failed manifest should make the job
fail or interrupt clearly without trying to process partial data as complete.

## Source Modes

M6B should support two input source modes:

- `path`
  The daemon reads an absolute file path on the daemon host and spools it into
  input chunks. The source file is not registered as a managed artifact.
- `upload`
  The HTTP client streams bytes to the daemon, and the daemon writes those bytes
  to input chunks while enforcing size and quota limits.

`external path without copy` can be explored later for local-only workflows, but
M6B should prefer spooling to give workers stable input even if the original
path changes while the job runs.

## Runtime Worker Contract

M6B should define the common handoff pattern, even though it does not execute a
model yet:

- producers write input chunks and then `input.done.toml`
- workers poll or watch for the done manifest
- workers translate byte chunks into workflow-specific model segments
- workers write result chunks and then `result.done.toml`
- readers stream or download result chunks by cursor until the done manifest is
  observed

Important boundary: byte chunks are transport/storage chunks, not necessarily
model-ready chunks. Audio and video runtimes may need a workflow-specific
segmenter that understands codecs, containers, timestamps, and frame/audio
boundaries.

## Daemon Runtime Slice

Add a daemon runtime module:

```text
src/tentgent-daemon/src/runtime/job_spool/
├── mod.rs
├── types.rs
├── layout.rs
├── writer.rs
├── reader.rs
└── gc.rs
```

File responsibilities:

- `types.rs`
  Pure types such as `JobSpoolSummary`, `JobSpoolManifest`,
  `SpoolStreamKind`, `SpoolState`, `SpoolCursor`, `SpoolLimits`, and
  `SpoolError`.
- `layout.rs`
  Path derivation from `job_id` to `runtime/jobs/<job_id>/spool`, chunk names,
  `.part` files, `.chunk` files, and terminal manifests.
- `writer.rs`
  Chunk writers for input and result streams, atomic finalization, manifest
  writes, byte counters, and optional hashing.
- `reader.rs`
  Cursor-based readers for input/result chunks. HTTP result streaming and
  download paths should share this layer.
- `gc.rs`
  TTL, quota, daemon-startup sweep, periodic sweep, terminal job cleanup, and
  orphan detection.

This layer is daemon-local runtime infrastructure. It should not introduce a
kernel `media` feature or managed media store in M6B.

Related files:

```text
src/tentgent-daemon/src/handlers/rest/spool/
├── mod.rs
└── dto.rs

src/tentgent-daemon/src/transport/rest/router.rs
src/tentgent-daemon/src/runtime/jobs/types.rs
src/tentgent-daemon/src/runtime/jobs/registry.rs
```

`handlers/rest/spool/mod.rs` owns route handlers. `dto.rs` owns request and
response DTOs so runtime spool types are not exposed directly as transport
types. `router.rs` wires routes. Job types and registry gain only the minimal
spool summary and job lifecycle hooks needed by `/v1/jobs`.

Do not add these in M6B:

- `src/tentgent-kernel/src/features/media/`
- `python/tentgent-daemon/...` runtime workers
- `src/tentgent-cli/src/cli/media.rs`
- a model/dataset-like `tentgent media` command group

## REST Slice

M6B can add plumbing routes for spool creation and inspection without adding a
public media catalog:

```text
POST /v1/spool/import/jobs
POST /v1/spool/upload/jobs
GET /v1/jobs/{job_id}
GET /v1/jobs/{job_id}/result
DELETE /v1/jobs/{job_id}/spool
```

Path import request:

```json
{
  "path": "/absolute/path/on/daemon-host/audio.wav",
  "media_type": "audio/wav",
  "expires_seconds": 86400
}
```

Upload request:

```text
multipart/form-data:
  file=@audio.wav
  media_type=audio/wav
  expires_seconds=86400
```

`POST /v1/spool/import/jobs` and `POST /v1/spool/upload/jobs` are base-layer
plumbing routes. Future workflow routes such as audio transcription jobs should
reuse the same spool internals but expose workflow-specific request names.

`GET /v1/jobs/{job_id}/result` should support a cursor or byte/chunk offset so
streaming readers can resume from the last emitted result chunk. It may return
`404 no_result` before a workflow writes results.

## Job Integration

Extend daemon job records with spool-aware metadata:

- `spool`: optional summary containing input/result states, total bytes, chunk
  counts, expiration, and cleanup state
- `JobKind::spool_import`
- `JobKind::spool_upload`
- `JobArtifact::new("job_spool").with_reference(job_id)` for plumbing jobs

Workflow jobs in M6C should use their own kinds, such as
`audio_transcription`, and attach the same spool summary rather than creating
new media ids.

Daemon stop behavior:

- `tentgent daemon stop` and daemon shutdown routes should request all running
  jobs to stop before process shutdown completes.
- Jobs that cannot finish cleanly should be marked `interrupted`, not silently
  abandoned.
- Active spool writers should finish or fail their current `.part` chunk, then
  write a failed/interrupted manifest when possible.
- After job interruption state is recorded, daemon stop should run one GC sweep.
- That GC sweep must honor retention buffers and must not delete a just-stopped
  job spool immediately.

## Cleanup And SSD Protection

M6B must make cleanup a first-class feature:

- default TTL for terminal job spools
- shorter TTL for failed/interrupted spools unless debug retention is requested
- retention buffer after successful completion
- retention buffer after a result has been read or downloaded
- retention buffer after stop, interruption, or failure
- maximum single-job input bytes
- maximum single-job result bytes
- maximum total spool bytes under `TENTGENT_HOME`
- opportunistic cleanup before creating a new spool
- daemon startup cleanup for orphaned or expired spools
- periodic daemon cleanup sweep while running

Cleanup must never delete an active running job's spool. If a job is stale
after daemon restart, the job should move to interrupted or failed before its
spool is eligible for cleanup. Completed, interrupted, failed, stopped, and
result-consumed jobs should all keep their spool for at least one configured
buffer window so future resume, retry, or delayed result readers have a stable
base to build on.

## CLI Slice

Do not add `tentgent media` as a model/dataset-like management surface in M6B.

CLI model workflows should be allowed to use the spool without exposing complex
job operations to the user. The preferred user shape for future CLI media model
commands is file in, output file out:

```bash
tentgent transcribe /path/to/audio.wav --output transcript.txt
tentgent image-generate --prompt "..." --output image.png
```

Internally, those commands may create a job, spool the input, continuously read
result chunks, and write the requested output path. The CLI should not require
users to manage `job_id` unless they explicitly ask for advanced inspection or
background behavior.

Optional low-level helpers can remain job-shaped if needed for debugging:

```bash
tentgent jobs inspect <job-id>
tentgent jobs result <job-id> --output result.bin
tentgent jobs cleanup
```

These helpers are lower priority than daemon REST and should not imply
long-lived media management.

## Documentation Slice

Add or update:

- `docs/contracts/job-spool.md`
- `docs/contracts/http-daemon.md`
- `docs/contracts/tentgent-daemon.md`
- `docs/contracts/runtime-home.md`
- `docs/user/commands.md` only after routes exist
- `AGENTS.md` only after the new contract document exists

Do not claim audio transcription, speech, vision chat, image generation, or
video model execution in user docs during M6B.

## Tests

Daemon runtime:

- chunk writer creates `.part` then atomically finalizes `.chunk`
- sequence numbers sort correctly
- manifests record chunk count, total bytes, media type, and terminal state
- result reader can read by chunk cursor or byte offset
- failed manifests prevent readers/workers from treating partial input as done
- GC skips active jobs and removes expired terminal/interrupted spools
- GC honors retention buffers for completed, consumed, interrupted, failed, and
  stopped spools
- quota checks reject oversized writes before unbounded disk growth
- daemon stop interrupts running spool jobs and runs one safe GC sweep

Daemon REST:

- path import job creates a job and spools input chunks
- upload job streams bytes into input chunks
- job inspect exposes bounded spool summary without raw filesystem internals
- result read returns clear `404 no_result` before result exists
- cleanup route removes only eligible job spool data
- daemon stop leaves just-interrupted spools available during their retention
  buffer
- invalid paths, oversized uploads, malformed multipart, and missing jobs map to
  existing REST error shapes

Workflow readiness:

- a fake worker can wait for `input.done.toml`, copy input chunks to result
  chunks, write `result.done.toml`, and let a reader stream the result by cursor
- a fake CLI workflow can read one input file, write result chunks, and mirror
  them into a requested output path without requiring manual job operations

## Non-Goals

- No managed `media_ref` catalog.
- No audio transcription, text-to-speech, vision chat, image generation, or
  video runtime execution.
- No OpenAI-compatible media routes.
- No WebSocket, WebRTC, resumable upload, or opaque raw stream proxy.
- No automatic media promotion to long-term storage.
- No transcoding, thumbnail generation, waveform extraction, or codec-aware
  segmentation beyond a fake worker test.

## Review Target

- Large file input can be written to a job-scoped spool from path or upload.
- Job inspect shows enough spool state to debug progress without exposing raw
  chunk paths as a public API.
- Result chunks can be written and read by cursor.
- Expired, failed, interrupted, or orphaned spools are cleaned safely.
- M6C can implement `audio-transcription` on top of the spool without inventing
  a separate media object store.
