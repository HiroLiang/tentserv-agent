# M6B Kernel Job Workspace Foundation

This is the focused execution plan for the second M6 slice in the
[capability-first release roadmap](./capability-first-release-roadmap.md).

Status: completed.

## Goal

- Move job and workspace semantics into `tentgent-kernel`.
- Keep daemon in-flight job management, worker scheduling, cancellation
  handles, detached process tracking, and long-running runtime loops in
  `tentgent-daemon`.
- Provide kernel ports for job records, job workspaces, chunk IO, result files,
  retention, and cleanup.
- Avoid a managed `media_ref` catalog for high-frequency media inputs.
- Keep public APIs workflow-shaped instead of exposing generic workspace or
  chunk plumbing.
- Keep daemon process lifecycle and model-bound server lifecycle outside the
  job catalog.

The current daemon-local file-backed chunk prototype proves the chunk and
manifest mechanics. Before M6C runtime work, that prototype should be migrated
behind kernel job workspace domain, ports, and use cases.

## Why This Replaces A Media Store

Audio, image, and video files can be large and frequent when Tentgent is used as
a tool runtime. Treating every input as a managed store object would create
unwanted SSD pressure and extra user-visible objects to clean up.

M6B should treat large media as job-local working data:

```text
feature request
        |
        v
kernel job workspace
        |
        v
input chunks / uploaded bytes / path-copied bytes
        |
        v
daemon worker / model runtime
        |
        v
result chunks and result file metadata
        |
        v
workflow result endpoint or CLI output file
        |
        v
retention and GC cleanup
```

A durable media catalog can be added later as an explicit "promote result" or
"save artifact" feature. It is not the default large-file path.

## Kernel Package Shape

Add a kernel feature package:

```text
src/tentgent-kernel/src/features/job/
|-- mod.rs
|-- domain.rs
|-- ports.rs
|-- usecases/
`-- infra/
```

`domain.rs` owns pure data:

- `JobId`, `JobKind`, `JobStatus`
- `JobTarget`, `JobArtifact`, `JobProgress`, `JobOutput`, `JobTiming`
- `JobWorkspaceRef`, `JobWorkspaceSummary`
- `JobStreamKind`: `input`, `result`, and future workflow-local streams
- `JobChunkCursor`, `JobChunkWrite`, `JobStreamSummary`
- `JobResultFile`, `JobResultFileList`
- `JobRetentionPolicy`, `JobQuotaPolicy`, `JobCleanupReport`

`ports.rs` owns the operational boundaries:

- `JobCatalogPort`
- `JobWorkspacePort`
- `JobChunkPort`
- `JobResultPort`
- `JobRetentionPort`

`usecases/` owns shared orchestration:

- create job record and workspace
- list jobs
- inspect one job
- transition job status
- update progress/output
- request cancellation
- delete a terminal job and workspace
- write/finalize input or result chunks
- list result files
- read result file bytes or chunks
- run retention-aware cleanup

Kernel job use cases should receive a resolved runtime layout. They should not
spawn daemon tasks, hold async loops, start Python, or own HTTP rendering.

## Two-Layer Job Model

M6B should split the job system into two explicit layers:

```text
kernel durable job layer
  job record, status, workspace ref, chunks, result files, retention

daemon in-flight job layer
  task handle, cancel token, detached process handle, scheduler queue
```

The daemon must keep an in-flight job manager because kernel code cannot hold
runtime handles for detached model pulls, adapter pulls, dataset synthesis,
LoRA workers, or future media workers. The daemon in-flight manager owns:

- spawning and supervising worker tasks
- tracking active `job_id` to task/process handles
- honoring cancellation requests when the workflow supports it
- marking jobs `running`, `succeeded`, `failed`, `interrupted`, or `canceled`
  through kernel job use cases
- marking durable queued/running records as `interrupted` or `failed` when the
  daemon starts and no recoverable runtime handle exists
- finalizing result files and bounded output through kernel ports before
  moving a job to a terminal status

The kernel owns the status vocabulary and legal transitions, but the daemon is
the authority for whether a worker is actually still alive.

Jobs are for detached or background one-shot work. The daemon process itself is
not a job. Model-bound servers should stay under the existing server lifecycle
surface because they are long-lived runtime resources, not one-shot jobs. Server
start, stop, readiness, and process metadata should remain server APIs. Only a
future detached server maintenance action with observable progress should be
considered for the job catalog.

## Port Responsibilities

`JobCatalogPort`:

- create a job for a workflow
- list jobs
- inspect one `job_id`
- transition queued/running/succeeded/failed/interrupted/canceled states
- record progress, bounded output tail, warning/result/error summaries
- mark cancellation requested
- delete job metadata when allowed

`JobWorkspacePort`:

- create or open a workspace for a job
- summarize workspace streams and retention state
- remove a terminal workspace
- run workspace sweeps
- enforce per-job and total workspace quota

`JobChunkPort`:

- write a `.part` chunk
- atomically commit the chunk
- finalize a stream manifest
- read chunks by cursor
- inspect stream state

`JobResultPort`:

- declare result files with `file_id`, name, MIME type, size, and format
- list result files for a job
- read one result file
- read result file chunks by cursor

`JobRetentionPort`:

- compute expiry windows
- preserve buffers after success, failure, interruption, cancellation, and
  result consumption
- produce cleanup reports for doctor/status diagnostics

## Standard Local Layout

The standard local implementation may store job data under:

```text
TENTGENT_HOME/
`-- runtime/
    `-- jobs/
        |-- <job_id>.json
        `-- <job_id>/
            `-- workspace/
                |-- input/
                |   |-- 0000000000000000.part
                |   `-- 0000000000000000.chunk
                |-- result/
                |   `-- 0000000000000000.chunk
                |-- input.done.toml
                |-- result.done.toml
                `-- results.toml
```

Rules:

- Chunk filenames are zero-padded hexadecimal sequence numbers.
- Writers write `.part`, fsync, then atomically rename to `.chunk`.
- Done manifests are the stream completion boundary.
- `results.toml` records workflow result files and their format/MIME metadata.
- Raw paths remain implementation details behind kernel ports.

## Daemon Boundary

`tentgent-daemon` owns runtime execution, not job semantics.

Daemon responsibilities:

- implement or wire file-backed kernel job workspace ports
- own an in-flight job manager for active task/process handles
- spawn and supervise local worker tasks
- map HTTP DTOs into kernel workflow use cases
- stream upload bytes into kernel chunk ports
- read kernel result files for HTTP responses
- request job interruption on daemon shutdown
- run a retention-aware cleanup sweep on shutdown/startup/periodic timers

Daemon must not expose generic workspace/chunk APIs and must not keep a
parallel durable job domain outside kernel. It may and should keep a
process-local in-flight registry that maps `job_id` to worker handles,
cancellation tokens, and detached process state. Existing
`src/tentgent-daemon/src/runtime/jobs` and `runtime/job_spool` code should be
split so durable job/workspace semantics move behind kernel ports while active
runtime supervision stays in daemon.

## Public API Boundary

Generic job APIs should be cross-workflow controls only:

```text
GET    /v1/jobs
GET    /v1/jobs/{job_id}
POST   /v1/jobs/{job_id}/cancel
DELETE /v1/jobs/{job_id}
```

Starting detached/background work belongs to feature endpoints:

```text
POST /v1/audio/transcriptions/jobs
POST /v1/audio/transcriptions/upload/jobs
POST /v1/audio/speech/jobs
POST /v1/images/generations/jobs
POST /v1/vision/chat/jobs
```

Result retrieval belongs to the feature endpoint that understands output
formats:

```text
GET /v1/audio/transcriptions/jobs/{job_id}/result
GET /v1/images/generations/jobs/{job_id}/files
GET /v1/images/generations/jobs/{job_id}/files/{file_id}
```

There must not be public `/v1/spool/*` routes. User-facing APIs should say
"transcribe this audio with this model", not "create a workspace".

## CLI Boundary

CLI foreground feature commands should be one-shot workflows, not durable job
records. They hide job/workspace mechanics and write their requested output
directly:

```bash
tentgent transcribe /path/to/audio.wav \
  --model-ref <audio-transcription-model-ref> \
  --output transcript.txt \
  --format text

tentgent image-generate \
  --model-ref <image-generation-model-ref> \
  --prompt "..." \
  --output image.png
```

Generic job helpers are controls, not launchers:

```bash
tentgent jobs ls
tentgent jobs inspect <job-id>
tentgent jobs cancel <job-id>
tentgent jobs delete <job-id>
```

The CLI may poll job status and read workflow result files internally, but it
should do that only for detached/background tasks. A foreground CLI command
should not appear in `tentgent jobs ls` unless the user explicitly asks for a
detached run or calls a daemon `/jobs` route.

## Execution Slices

### M6B.1 Kernel Job Domain And Ports

- Add `features/job` domain types.
- Define `JobCatalogPort`, `JobWorkspacePort`, `JobChunkPort`,
  `JobResultPort`, and `JobRetentionPort`.
- Add use-case request/response structs for list, inspect, cancel, delete,
  workspace open/remove, chunk IO, and result file access.

### M6B.2 Move Job Registry Semantics To Kernel

- Move daemon-local job record types into kernel job domain.
- Move persistence shape into kernel-owned use cases and ports.
- Keep daemon as the standard runtime host that wires the port implementation.
- Preserve existing `/v1/jobs` behavior through the daemon adapter.

### M6B.3 Move Workspace And Chunk Semantics Behind Kernel Ports

- Replace daemon-local spool vocabulary with kernel workspace vocabulary.
- Expose open/remove/write/read/finalize/list-result operations as kernel
  ports.
- Keep file-backed implementation under the standard local adapter.
- Rename internal layout from `spool/` to `workspace/` unless migration cost
  argues for a compatibility alias.

### M6B.4 Daemon In-Flight Job Manager

- Keep a daemon-local runtime registry for active jobs.
- Track `job_id` to async task handles, cancellation tokens, and detached child
  process handles where needed.
- Reconcile durable queued/running job records on daemon startup.
- Mark orphaned non-recoverable records `interrupted` or `failed` through
  kernel job use cases.
- Ensure worker completion writes final progress/result/error state through
  kernel job use cases.
- Exclude the daemon process and long-lived model-bound server processes from
  the job registry.

### M6B.5 Daemon Wiring And Shutdown

- Wire daemon REST job handlers through kernel job use cases.
- Make daemon shutdown mark active jobs interrupted through kernel operations.
- Run one retention-aware cleanup sweep after interruption state is recorded.
- Ensure active workspace deletion is rejected.

### M6B.6 Tests

Kernel tests:

- job status transitions
- list and inspect behavior
- workspace open/remove lifecycle
- chunk write/commit/finalize/read cursor behavior
- result file declaration/list/read
- retention buffer and cleanup rules
- quota rejection before unbounded disk growth

Daemon tests:

- `/v1/jobs` uses kernel job use cases
- cancel/delete map to kernel job operations
- in-flight registry tracks active worker handles
- startup reconciliation marks orphaned running jobs interrupted or failed
- worker completion records terminal state through kernel use cases
- shutdown interrupts active jobs and runs retention-aware cleanup
- no `/v1/spool/*` route exists

## Non-Goals

- No managed `media_ref` catalog.
- No public workspace/chunk plumbing API.
- No audio transcription, text-to-speech, vision chat, image generation, or
  video runtime execution in M6B.
- No OpenAI-compatible media routes.
- No WebSocket, WebRTC, resumable upload, or opaque raw stream proxy.
- No automatic media promotion to long-term storage.
- No daemon lifecycle jobs.
- No model-bound server lifecycle jobs in M6B.

## Review Target

- Kernel owns job and workspace semantics through ports and use cases.
- Daemon owns in-flight job management, runtime scheduling, worker handles,
  shutdown reconciliation, and HTTP adaptation.
- Feature endpoints can start jobs and use the shared workspace foundation.
- Users see workflow APIs and job controls, not chunk/workspace plumbing.
