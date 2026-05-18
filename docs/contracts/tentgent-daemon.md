# Tentgent Daemon

This document defines the Rust daemon application host boundary.

## Scope

`src/tentgent-daemon/` is the long-running Rust process that owns app
bootstrap, transport listeners, daemon-local state, and background runtime
systems.

The crate should stay thin over `tentgent-kernel` use cases. It may keep
process-local services such as caches, schedulers, job registries, and transport
handler wiring, but product behavior should remain in kernel domain, ports,
infrastructure, and use cases.

## Dependency Direction

- `tentgent-daemon` depends on `tentgent-kernel`.
- `tentgent-daemon` may temporarily call older crates only through explicit
  migration bridges.
- `tentgent-kernel` must not depend on `tentgent-daemon`.
- `tentgent-cli` may launch or control the daemon process, but daemon request
  handling should live in `tentgent-daemon`.
- `tentgent-http` is the legacy HTTP entrypoint and should be treated as a
  migration source, not the final daemon architecture.
- `python/tentgent-daemon` remains the Python model runtime/backend subproject
  until that lower-level adapter is redesigned.

## Module Shape

- `src/main.rs`
  CLI-shaped process entrypoint for starting the daemon host.
- `src/bootstrap/`
  Builds logging, config, kernel adapter bundles, daemon services, and app
  state.
- `src/app/`
  Owns `DaemonApp`, shared app state, and service registry accessors.
- `src/kernel/`
  Owns daemon-local composition of kernel infrastructure components and exposes
  use-case builders to app services.
- `src/transport/`
  Owns long-running listeners such as REST, local sockets, or future streaming
  transports.
- `src/handlers/`
  Maps transport DTOs into daemon app services and kernel use cases.
- `src/runtime/`
  Owns daemon-local cache, scheduler, job registry, and future memory-like
  process state.

## Bootstrap Boundary

Daemon startup should be split into stable steps:

1. Parse process arguments or external config.
2. Resolve runtime layout enough to find `logs_dir`.
3. Initialize logging and tracing.
4. Build kernel infrastructure bundles.
5. Build daemon-local runtime systems.
6. Build transport entrypoints.
7. Run the enabled transports until shutdown.

Startup code should not embed route behavior. Route behavior belongs under
`handlers/`, with kernel-facing work delegated to app services.

## Logging Boundary

Daemon structured logs are written through `tracing`. The file sink should use
`RuntimeLayout.logs_dir` from the kernel layout resolver instead of hard-coded
paths.

The daemon tracing log uses a rolling `daemon.log` prefix under `logs/`.
Detached-process stdout and stderr files such as `daemon.stdout.log` and
`daemon.stderr.log` remain lifecycle-launch artifacts, not the primary
structured application log.

## Kernel Component Boundary

`src/kernel/` is allowed to know which concrete kernel infrastructure structs
compose a feature. Code outside this daemon composition layer should prefer
use-case builders such as `models().catalog_usecase()` or
`server_usecase()` instead of directly constructing filesystem stores, probes,
or runtime clients.

Cross-feature use cases should be built at the component registry level so
handlers do not need to know supporting dependencies. For example, chat can
receive a chat use case while the registry wires runtime resolution, model
resolution, adapter compatibility, and runtime execution behind it.

## Transport Boundary

REST is one transport entrypoint, not the daemon architecture itself. Future
local sockets or internal control channels should be added under `transport/`
and wired through the same `DaemonAppState`.

The REST transport uses `axum` directly inside `src/transport/rest/` and
`src/handlers/rest/`. Axum types should not leak into `app/`, `kernel/`, or
kernel use cases.

Transport handlers should:

- Parse request DTOs.
- Call daemon services or kernel use cases.
- Map domain results to response DTOs.
- Avoid owning persistence or runtime capability decisions directly.

REST response DTOs should live beside the handler that owns that API surface
(`handlers/rest/<feature>/dto.rs` for larger features, or the handler file for
tiny endpoints). `transport/rest/response.rs` should stay limited to truly
shared response primitives such as the service name and standard error shape, so
new API groups do not grow a global DTO file.

Provider-compatible chat routes are local-model facades, not cloud proxy
routes. They accept provider-shaped request and response envelopes, then map
text-only prompts into the kernel chat use cases. The compatible `model` value
may be a managed model ref, a unique model-ref prefix, a stored Hugging Face
`source_repo`, or the final repository name when it resolves uniquely. Token
usage is reported as unknown (`null`) until the runtime returns authoritative
counts. Tool calls, images, audio, and other multimodal parts must be rejected at
the adapter boundary until kernel chat domain types and use cases explicitly
support them.

The first stable REST surface is:

- `GET /healthz`
  Lightweight process health response.
- `GET /v1/status`
  Kernel-backed daemon status response.
- `GET /v1/jobs`
  Daemon-runtime job list response. Jobs expose daemon-local execution state
  for long-running one-shot work, including status, stage, bounded progress,
  bounded output tail, target, artifact, warnings, result summary, and error
  summary.
- `GET /v1/jobs/{job_id}`
  Daemon-runtime job inspection response for one persisted or in-memory job.
- `POST /v1/chat`
  Kernel-backed local model chat. The request selects a managed `model_ref`,
  optional `adapter_ref`, chat messages, generation options, and optional SSE
  streaming. Session-aware chat remains pending daemon runtime work.
- `POST /v1/chat/completions`
  OpenAI Chat Completions-compatible local model chat adapter. The request uses
  `model`, `messages`, generation options, and optional `stream`; the handler
  maps the request into the same kernel chat use cases as `/v1/chat`.
- `POST /v1/messages`
  Claude Messages-compatible local model chat adapter. Text-only message
  content and optional SSE streaming are mapped into the same kernel chat use
  cases as `/v1/chat`.
- `POST /v1beta/models/{model}:generateContent`
  Gemini GenerateContent-compatible local model chat adapter for text-only
  content. `{model}` is resolved as a managed local model reference or alias.
- `POST /v1beta/models/{model}:streamGenerateContent`
  Gemini streamGenerateContent-compatible SSE adapter for text-only content.
  `{model}` is resolved as a managed local model reference or alias.
- `GET /v1/models`
  Kernel-backed model catalog list response.
- `GET /v1/models/{reference}`
  Kernel-backed model inspection response for a full model ref or unique
  prefix. Model DTOs expose `model_capabilities` and
  `model_capability_source` from kernel metadata so chat, embedding, and rerank
  support remains visible at the API boundary.
- `DELETE /v1/models/{reference}`
  Removes a stored model by full model ref or unique prefix after kernel
  reference checks pass.
- `POST /v1/models/import/jobs`
  Starts a daemon background job that imports a local model path into the
  kernel model store.
- `POST /v1/models/pull/jobs`
  Starts a daemon background job that pulls a Hugging Face model into the
  kernel model store. The route validates the request, creates a job record,
  returns `202 Accepted`, and reports pull progress through `/v1/jobs`.
- `GET /v1/adapters`
  Kernel-backed adapter catalog list response.
- `GET /v1/adapters/{reference}`
  Kernel-backed adapter inspection response for a full adapter ref or unique
  prefix. Adapter DTOs expose base-model binding hints, backend support, source
  metadata, and optional training provenance from kernel metadata.
- `DELETE /v1/adapters/{reference}`
  Removes a stored adapter by full adapter ref or unique prefix after kernel
  reference checks pass.
- `POST /v1/adapters/import/jobs`
  Starts a daemon background job that imports a local adapter path into the
  kernel adapter store, optionally binding it to a managed base model.
- `POST /v1/adapters/pull/jobs`
  Starts a daemon background job that pulls a Hugging Face adapter into the
  kernel adapter store and reports pull progress through `/v1/jobs`.
- `GET /v1/datasets`
  Kernel-backed dataset catalog list response.
- `GET /v1/datasets/{reference}`
  Kernel-backed dataset inspection response for a full dataset ref or unique
  prefix. Dataset DTOs expose tuning readiness, split paths, warnings, source
  metadata, and managed source paths from kernel metadata.
- `DELETE /v1/datasets/{reference}`
  Removes a stored dataset by full dataset ref or unique prefix after kernel
  reference checks pass.
- `POST /v1/datasets/import/jobs`
  Starts a daemon background job that imports a local dataset path into the
  kernel dataset store.
- `POST /v1/datasets/synth/jobs`
  Starts a daemon background job for provider-backed dataset synthesis. Provider
  auth and Python runtime execution happen inside the job.
- `POST /v1/datasets/eval/jobs`
  Starts a daemon background job for provider-backed dataset evaluation.
- `GET /v1/train/lora/plans`
  Kernel-backed LoRA train plan list response.
- `POST /v1/train/lora/plans/preview`
  Builds a normalized LoRA train plan preview without writing it.
- `POST /v1/train/lora/plans`
  Creates or reuses a normalized LoRA train plan.
- `GET /v1/train/lora/plans/{reference}`
  Kernel-backed LoRA train plan inspection response for a full plan ref or
  unique prefix.
- `DELETE /v1/train/lora/plans/{reference}`
  Removes a saved LoRA train plan when it has no run records.
- `GET /v1/train/lora/runs`
  Kernel-backed list response for all LoRA train runs.
- `GET /v1/train/lora/plans/{reference}/runs`
  Kernel-backed list response for runs under one LoRA train plan.
- `POST /v1/train/lora/plans/{reference}/runs`
  Starts a daemon background job for a LoRA train run from a saved plan. The job
  creates the kernel run record, launches the detached worker, then polls the
  run record until it reaches a terminal status.
- `GET /v1/train/lora/runs/{reference}`
  Kernel-backed LoRA train run inspection response for a full run ref or unique
  prefix.
- `GET /v1/train/lora/runs/{reference}/metrics`
  Reads a bounded tail of JSON metric events for one LoRA train run. The
  optional `tail` query parameter defaults to 200 and is capped at 1000 events.
- `GET /v1/train/lora/runs/{reference}/logs`
  Reads raw-log metadata for one LoRA train run.
- `GET /v1/train/lora/runs/{reference}/logs/raw`
  Reads a bounded raw-log tail for one LoRA train run. The optional
  `tail_bytes` query parameter defaults to 65536 and is capped at 262144 bytes.
- `GET /v1/servers`
  Kernel-backed stored server list response with process-state observation.
- `GET /v1/servers/{reference}`
  Kernel-backed stored server inspection response for a full server ref or
  unique prefix. Server DTOs expose runtime target, bind settings, process
  metadata, and server-local paths from kernel state.
- `GET /v1/sessions`
  Kernel-backed session catalog list response.
- `GET /v1/sessions/{reference}`
  Kernel-backed session inspection response for a full session ref or unique
  prefix.
- `GET /v1/sessions/{reference}/messages`
  Kernel-backed recent transcript response. The optional `tail` query parameter
  defaults to 200 and is capped at 1000 messages.

## Runtime Boundary

Daemon-local runtime state is allowed when it is process-scoped:

- Memory cache.
- Job registry.
- Scheduler.
- Connection or session bookkeeping.

Persistent state and product decisions should remain in `tentgent-kernel`
unless the state is explicitly transport-only.

The daemon runtime layer may define typed records for daemon execution state,
such as background job status, progress, bounded output tails, affected targets,
and produced artifacts. These types are daemon runtime models, not kernel
feature domain. They may reference kernel-owned artifacts such as `model_ref`,
`adapter_ref`, `dataset_ref`, `session_ref`, or LoRA `run_ref`, but product
validation and store mutations stay behind kernel use cases.

Long-running one-shot work should be represented as jobs when the caller needs
to keep observing progress after the initial request returns. Job records should
include enough state to restore operator visibility after navigation or process
restart:

- stable job id, kind, status, stage, and timestamps
- target section/ref/path affected by the operation
- produced artifact ref/path when available
- bytes/files progress, percent, speed, and ETA when available
- bounded redacted output tail plus optional daemon-host raw log path
- warning, result, and error summaries

The daemon session manager should coordinate session-aware chat and compaction
above kernel session use cases. It may hold per-session locks, in-memory context
caches, and compaction policy, but persisted session metadata and transcripts
remain kernel session-store data.
