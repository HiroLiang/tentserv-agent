# Tentgent Daemon Runtime Systems

This plan tracks the `src/tentgent-daemon/` runtime systems that sit above
kernel use cases but below transports. The goal is to keep REST handlers thin
while giving the long-running daemon a durable way to manage background work,
session orchestration, and future local process state.

## Direction

`tentgent-daemon` should own daemon-local orchestration only. Product rules,
store schemas, refs, and cross-feature validation stay in `tentgent-kernel`.
The daemon runtime layer may keep typed records for daemon execution state,
but those records are not kernel feature domain.

Two runtime systems are planned:

- Job Engine for one-shot long-running work.
- Session Manager for session-aware daemon chat and transcript compaction.

## Job Engine

The Job Engine manages one-shot long-running work such as model pulls, adapter
pulls, dataset import/synthesis/evaluation, LoRA train runs, and session
compaction jobs.

Core responsibilities:

- create a stable `job_id` quickly and return it from `/jobs` mutation routes
- persist bounded job records under the resolved runtime home
- report status, stage, timestamps, progress, affected targets, artifacts,
  warnings, result/error summaries, and bounded output tails
- keep raw logs or large task output behind explicit daemon-host log paths
  instead of embedding unbounded output in the job record
- mark active jobs `interrupted` after daemon restart in the first durable
  implementation
- avoid fake cancellation; expose `cancellable` only when a runner can actually
  stop the underlying process

The job record is a daemon runtime type, not a kernel product domain object.
It may point at kernel-owned artifacts by reference:

```text
JobItem
  job_id
  kind = model_pull | adapter_pull | dataset_import | lora_train_run | session_compaction | ...
  status = queued | running | succeeded | failed | interrupted | canceled
  target = section/ref/path affected by the operation
  artifact = produced model_ref, adapter_ref, train run, report path, etc.
  progress = bytes/files/percent/speed/eta when available
  output = bounded redacted display tail plus optional raw log path
```

Initial implementation slices:

1. Define job runtime types with progress and output fields.
2. Expand `JobRegistry` into an in-memory registry with persistence-ready
   mutation methods.
3. Add `GET /v1/jobs` and `GET /v1/jobs/{job_id}` to the new daemon.
4. Add a `JobRunner` for spawning background tasks and updating the registry.
5. Move store mutation jobs onto the runner as the first proof of progress
   mapping.
6. Move LoRA train runs onto the runner, linking job state to the kernel run
   record and worker lifecycle.

Current state:

- Job runtime types and in-memory registry mutation methods are in place.
- Job records persist under the daemon runtime jobs directory.
- Read-only `GET /v1/jobs` and `GET /v1/jobs/{job_id}` routes are in place.
- Background runner execution is in place for blocking job tasks.
- Model import/pull, adapter import/pull, dataset import/synthesis/evaluation,
  and LoRA train run start routes create job records before doing long-running
  work.
- Hugging Face pulls map download progress into job progress. Dataset synth/eval
  jobs expose runtime start/completion and bounded progress output. LoRA train
  jobs launch the worker and poll the kernel run record until terminal status.

## Session Manager

The Session Manager coordinates daemon session workflows that require more than
one kernel session use-case call. It does not replace the kernel session store
or session use cases.

Core responsibilities:

- serialize mutations per `session_ref` so append, chat, and compaction do not
  race
- prepare session chat context through kernel session use cases
- turn `SessionSummaryRequirement` values into actual chat summarization work
  by using daemon-selected chat infrastructure
- apply rolling, persisted, or request-context summaries back through kernel
  session use cases
- expose manual/background compaction as jobs when it may run longer than a
  request should block
- keep optional hot context cache in daemon memory without making it the source
  of truth

Planned shape:

```text
runtime/sessions/
  manager.rs    facade used by REST/chat handlers
  gate.rs       per-session mutation lock
  compactor.rs  requirement -> chat summary -> apply summary
  policy.rs     thresholds and sync/async decisions
  cache.rs      optional hot context cache
```

The current daemon `SessionSummaryGenerator` intentionally returns an error
that summary generation must be handled by a chat handler. The Session Manager
is the daemon-local place to provide that bridge without pushing chat transport
or daemon jobs into kernel.

## API Direction

Read-only catalog routes can call kernel use cases directly. Mutations that may
take noticeable time should use explicit `/jobs` routes and return `202` with a
job record. Short synchronous mutations may stay synchronous when they do not
hide background work.

Examples:

- `POST /v1/models/pull/jobs`
- `POST /v1/models/import/jobs`
- `POST /v1/adapters/pull/jobs`
- `POST /v1/adapters/import/jobs`
- `POST /v1/datasets/import/jobs`
- `POST /v1/datasets/synth/jobs`
- `POST /v1/datasets/eval/jobs`
- `POST /v1/train/lora/plans/{ref}/runs`
- `POST /v1/sessions/{ref}/compact/jobs`

Existing response DTOs remain transport-owned under `handlers/rest/`. Job DTOs
should map from daemon runtime job types rather than leaking axum or handler
state into the runtime layer.
