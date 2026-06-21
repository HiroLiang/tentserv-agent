# Job Workspace

This document defines the kernel-owned job and workspace boundary for
long-running workflows and large binary inputs or outputs.

## Scope

- Put job identity, status, workspace, chunk, result-file, and cleanup
  semantics in `tentgent-kernel`.
- Keep daemon in-flight job management, worker scheduling, cancellation
  handles, and process lifetime in `tentgent-daemon`.
- Let feature use cases start work through workflow-specific APIs, not through
  generic workspace plumbing or job-start routes.
- Keep large temporary inputs and outputs out of the managed model, adapter,
  dataset, and session stores.
- Keep daemon process lifecycle and model-bound server lifecycle out of the job
  catalog.

The kernel owns the contract. The daemon may provide the standard file-backed
implementation and run workers, but callers should program against kernel
domain types, use cases, and ports.

## Kernel Ownership

Add a kernel feature package:

```text
src/tentgent-kernel/src/features/job/
|-- domain.rs
|-- ports.rs
|-- usecases/
`-- infra/
```

Kernel domain should own:

- `JobId`, `JobKind`, `JobStatus`
- job target, artifact, progress, output tail, timing, warning/result/error
  summaries
- `JobWorkspaceRef`
- `JobWorkspaceSummary`
- `JobStreamKind`: `input`, `result`, and future workflow-local streams
- `JobChunkCursor`
- `JobChunkWrite`
- `JobResultFile`
- `JobResultFileList`
- retention and quota policy data

Kernel ports should own operations such as:

- create or open a job workspace
- create a job record for a workflow
- list jobs
- inspect one `job_id`
- update status, progress, output, and summaries
- request cancellation or interruption
- remove a terminal job workspace
- write input/result chunks
- finalize input/result streams
- read chunks by cursor
- list result files
- read a result file or result file chunk
- run cleanup or quota sweeps

The kernel still must not own an always-running runtime loop. It defines the
operations; the daemon decides when a worker task is spawned, stopped,
interrupted, or reaped.

## Runtime Ownership Model

The job system has two layers:

- Kernel durable layer
  Owns job records, status transitions, workspace references, chunk cursors,
  result file metadata, retention policy, and cleanup rules.
- Daemon in-flight layer
  Owns active task handles, cancellation tokens, detached process handles,
  scheduler queues, worker supervision, and recovery decisions when a durable
  job record has no running worker.

The daemon must manage running jobs because kernel code cannot hold handles for
detached model pulls, adapter pulls, dataset jobs, LoRA train workers, or future
media workers. When a daemon worker starts, finishes, fails, is interrupted, or
loses its runtime handle, daemon code must record that outcome through kernel
job use cases. On daemon startup, queued or running durable records without a
recoverable worker should be marked `interrupted` or `failed` according to the
workflow contract before cleanup becomes eligible.

Jobs represent one-shot background work. The daemon process itself is never a
job. Model-bound servers should remain server lifecycle resources with their own
stored specs, process metadata, health/readiness, and start/stop APIs. A future
server maintenance action can become a job only if it is a detached one-shot
operation that needs progress observation after the initiating request returns.

## Suggested Ports

Keep ports narrow enough that feature use cases can depend on stable behavior
without knowing daemon internals:

```text
JobCatalogPort
  create_job
  list_jobs
  inspect_job
  transition_job
  update_progress
  append_output
  mark_cancellation_requested
  delete_job_record

JobWorkspacePort
  open_workspace
  summarize_workspace
  remove_workspace
  sweep_workspaces
  check_workspace_quota

JobChunkPort
  write_chunk
  commit_chunk
  finalize_stream
  read_chunks
  inspect_stream

JobResultPort
  declare_result_file
  list_result_files
  read_result_file
  read_result_file_chunks
```

Feature-specific use cases, such as audio transcription, should use these ports
to prepare input, publish results, and expose output metadata. They should not
reach into daemon-local path logic directly.

## Standard Layout

The standard local implementation may store workspaces under the runtime job
root:

```text
TENTGENT_HOME/
`-- runtime/
    `-- jobs/
        |-- <job_id>.json
        `-- <job_id>/
            `-- workspace/
                |-- input/
                |   `-- 0000000000000000.chunk
                |-- result/
                |   `-- 0000000000000000.chunk
                |-- results.toml
                |-- input.done.toml
                `-- result.done.toml
```

Rules:

- Chunk filenames are zero-padded hexadecimal sequence numbers.
- Writers write `<sequence>.part`, fsync the file, then rename it to
  `<sequence>.chunk`.
- A stream is complete only after its terminal manifest exists.
- Raw workspace paths and chunk cursors are internal implementation details.
  Public APIs expose workflow operations and result files.

## API Boundary

There must not be public `/v1/spool/*` routes. Generic job APIs should stay
cross-workflow:

```text
GET    /v1/jobs
GET    /v1/jobs/{job_id}
POST   /v1/jobs/{job_id}/cancel
DELETE /v1/jobs/{job_id}
```

`POST /v1/jobs/{job_id}/cancel` marks a non-terminal durable job record
`canceled`. For daemon blocking workers, cancellation also asks the in-flight
task handle to abort, but already-started blocking work may continue outside
the durable job state. This is best-effort worker interruption, not a guarantee
that every underlying runtime operation stops immediately.

`DELETE /v1/jobs/{job_id}` is terminal-only. It must remove the durable job
record and the job workspace when that workspace exists. Active jobs must
return conflict instead of deleting their workspace.

Starting detached/background work belongs to a feature endpoint:

```text
POST /v1/audio/transcriptions/job
POST /v1/audio/speech/job
POST /v1/images/generations/job
POST /v1/images/transforms/job
POST /v1/images/inpaint/job
POST /v1/images/control/job
POST /v1/video/understanding/job
```

Result retrieval should be owned by the workflow that understands the output
format:

```text
GET /v1/audio/transcriptions/job/{job_id}/result
GET /v1/audio/speech/job/{job_id}/result
GET /v1/images/generations/job/{job_id}/files
GET /v1/images/generations/job/{job_id}/files/{file_id}
GET /v1/images/transforms/job/{job_id}/files
GET /v1/images/transforms/job/{job_id}/files/{file_id}
GET /v1/images/inpaint/job/{job_id}/files
GET /v1/images/inpaint/job/{job_id}/files/{file_id}
GET /v1/images/control/job/{job_id}/files
GET /v1/images/control/job/{job_id}/files/{file_id}
GET /v1/video/understanding/job/{job_id}/result
```

The kernel may expose generic result-file use cases and ports, but public HTTP
routes should remain workflow-shaped unless a generic debug surface is
explicitly approved.

## Daemon Boundary

`tentgent-daemon` may own:

- axum request parsing, multipart handling, and response rendering
- local task spawning and cancellation signaling
- in-flight job registry and scheduler queues
- detached child process handles for workers that outlive one request
- process shutdown coordination
- standard file-backed implementations of job workspace ports
- Python runtime invocation for workflow workers

`tentgent-daemon` must not define a parallel durable job domain that bypasses
kernel ports. Its in-flight registry may reference `job_id`, worker handles,
and cancellation state, but persisted job state and workspace mutation must go
through kernel job use cases. Existing daemon-local job registry and
file-backed chunk code should be treated as a prototype to split into daemon
runtime supervision plus kernel job workspace ports.

## Cleanup

Cleanup must never delete a running job workspace. Deleting or cleaning up a
job should go through kernel job/workspace use cases so retention buffers,
quota policy, interrupted state, and result-consumed state stay consistent
across CLI and daemon REST.

Daemon shutdown should mark active jobs interrupted through kernel job
operations, then run one retention-aware sweep. The sweep must not delete
just-interrupted or just-completed workspaces immediately.
