# Post-M7 Runtime Compatibility Architecture

Status: future marker, not initialized for execution.

This plan records architecture problems discovered during M6 media backend work.
It is intentionally separate from the current M6-to-M7 release track. Do not use
this plan to block M7 signing, notarization, or release pipeline work.

Post-M7 initialization note: when this marker becomes executable work, rename the
plan to reflect the broader compatibility-management architecture, for example
`post-m7-model-adapter-compatibility-management.md` or a similarly explicit
name. The future plan should cover model and LoRA adapter compatibility
management, runtime routing, media-serving wrappers, runtime stream proxy
decisions, and the metadata/proof store that makes those decisions durable.
SQLite is one implementation tool for the durable state layer; it is not the
whole architecture.

## Purpose

M6 adds concrete media workflows and first runtime paths. That is useful, but it
does not yet make Tentgent a complete cross-runtime compatibility layer for
arbitrary Hugging Face, MLX, Diffusers, or future local model packages.

After M7, open dedicated execution plans for a better compatibility system that
can explain, test, cache, and share which models really work with which runtime
adapters on a given machine.

## Known Problems To Solve Later

- Model file layout is not enough to prove capability or runtime compatibility.
- A model repo can advertise the right broad family but still miss runtime
  metadata such as `preprocessor_config.json`, `tokenizer.json`, or
  `generation_config.json`.
- Tentgent must not synthesize or patch processor/tokenizer metadata just to make
  a smoke test pass. If a runtime requires those files, compatibility must come
  from the model package or from an explicit future conversion/import workflow.
- Different runtime adapters can sometimes drive the same conceptual model
  family, but the current resolver chooses one path with limited evidence.
- Runtime errors need to become durable compatibility evidence instead of
  disappearing as one failed command.
- Model and adapter pull/import metadata are declarations or hints, not proof
  that a capability, backend, or adapter binding can run.
- Adapter binding should distinguish "declared for this model" from "verified
  with this model, capability, backend, runtime version, and selected weight
  file".
- Model capability assignment should distinguish "declared or inferred" from
  "runtime-verified". A model can have multiple declared capabilities and
  multiple verified runtime profiles.
- LoRA adapter classification should distinguish file format, intended target
  capability, compatible backend family, selected weight file, trigger hints,
  base-model binding, and runtime-verified binding.
- A LoRA adapter verified with one model/backend/capability tuple must not be
  treated as verified with another tuple unless that exact tuple has its own
  proof.
- A filesystem-only catalog becomes awkward once Tentgent needs durable
  verified, failed, warning, and stale runtime evidence across models,
  adapters, jobs, and backend versions.
- Operators need optional runtime capacity controls, but read-only inference
  should remain concurrent by default.
- Some long-lived server capabilities may need explicit wrapping rather than
  direct Python route exposure. For example, future `vision-chat` and
  `audio-transcription` serving must decide whether media upload, temp-file
  cleanup, and route-family dispatch belong in the Python server process, a
  daemon sidecar wrapper, or a separate serving runtime.
- Runtime stream proxying needs typed route contracts. A generic byte tunnel
  would leak backend-specific protocols and blur Tentgent's stable API
  boundary.
- A future shared compatibility registry may help users avoid repeating known
  broken model/runtime combinations, but local runtime versions and local probes
  must remain authoritative.

## Future Work Areas

### Compatibility Probe And Cache

- Validate a stored model against one or more candidate runtime adapters with
  lightweight smoke requests where practical.
- Cache evidence by model identity, source revision, capability, runtime family,
  runtime package version, platform, selected adapter, and input/output shape.
- Record successful, warning, and incompatible states in a way that CLI inspect,
  model list, doctor, daemon DTOs, and runtime errors can show.
- Treat missing metadata, unsupported generation config, unsupported media input
  format, runtime import failure, and decode failure as structured evidence.
- Record model proof at the granularity of `model_ref`, capability, backend,
  runtime family, runtime version, platform, and relevant input/output shape.
- Record adapter proof at the granularity of `adapter_ref`, `model_ref`,
  capability, backend, selected weight file, runtime family, runtime version,
  and scale/configuration when it affects execution.
- Treat successful execution as a verified proof for that exact tuple only. Do
  not treat a verified chat path as image-generation proof, or an adapter
  verified on one base model as proof for another base model.
- Track proof states such as `declared`, `inferred`, `bound`, `verified`,
  `failed`, and `stale`.

### Model And LoRA Compatibility Management

- Treat static import and pull metadata as compatibility hints only. Static
  facts include model source, file layout, config files, declared capabilities,
  adapter format, selected adapter weight file, and backend support hints.
- Add explicit compatibility records for:
  - model capability declarations and manual corrections
  - candidate runtime driver support
  - model-to-runtime verification results
  - adapter-to-base-model binding intent
  - adapter-to-model runtime verification results
  - known failed tuples and their structured reasons
- Model compatibility should be keyed by at least:
  - `model_ref`
  - capability
  - backend/runtime family
  - runtime package version
  - platform/device class
  - relevant input/output shape
- LoRA compatibility should be keyed by at least:
  - `adapter_ref`
  - `model_ref`
  - target capability
  - backend/runtime family
  - selected adapter weight file
  - runtime package version
  - optional scale/config values when they affect loading
- Keep adapter binding separate from adapter verification:
  - `bound` means the adapter is intended for a model or model source.
  - `verified` means the adapter successfully executed with that exact
    model/runtime/capability tuple.
  - `failed` means Tentgent tried that tuple and captured a structured reason.
  - `stale` means a runtime, model, adapter, or schema change invalidated old
    evidence.
- Require explicit re-verification before trusting a LoRA adapter on a different
  model, different capability, different backend, or different selected weight
  file.
- Surface compatibility state in CLI inspect/list, daemon DTOs, doctor output,
  and runtime error messages so users can see whether a route is declared,
  inferred, verified, failed, or stale.

### SQLite Metadata And Proof Store

- Keep large content in the filesystem stores:
  - model snapshots
  - adapter snapshots and weight files
  - datasets
  - job workspaces and media artifacts
- Add a SQLite-backed metadata/proof layer for query-heavy relationships and
  state:
  - model capability declarations and corrections
  - runtime verification profiles
  - adapter bindings
  - adapter/model/backend proof records
  - structured failure and warning evidence
  - job metadata and cleanup state where it benefits from indexed queries
- Start as an index/proof layer beside the existing filesystem source of truth,
  not as a wholesale replacement for model or adapter content stores.
- Make the index rebuildable from filesystem metadata where possible, while
  preserving runtime proof records that cannot be derived from static files.
- Use schema/versioning rules so runtime upgrades can mark affected proof rows
  stale instead of silently trusting old evidence.
- Keep the door open for later migration of catalog/list/query paths to SQLite
  after the proof layer is stable.

### Dynamic Runtime Transduction

- Add an explainable routing layer between stored model metadata and concrete
  runtime adapters.
- Let Tentgent evaluate bounded candidate drivers when a model may be compatible
  with multiple runtime families.
- Preserve user-facing capability contracts. Users should call workflows such as
  `transcribe`, `vision chat`, image generation jobs, or future speech/video
  jobs; Tentgent should explain which backend was selected or rejected.
- Avoid unbounded trial-and-error. Runtime candidate attempts should be
  documented, cached, and easy to inspect.

### Shared Compatibility Registry Decision

- Decide whether Tentgent should publish or consume a shared compatibility
  registry for model/runtime/capability records.
- Keep local operation authoritative. Shared records can provide hints, not
  replace local probe results.
- Define trust, versioning, invalidation, opt-in behavior, and privacy boundaries
  before implementing online sharing.
- Distinguish "community hint" from "verified on this machine".

### Model Resource Coordination

- Add model/resource leases only where needed: model mutation, adapter writes,
  conversion, exclusive cache updates, server warm-model ownership, or backends
  that cannot safely share runtime state.
- Keep read-only inference concurrent by default unless a backend or operator
  setting defines stricter capacity limits.
- Add daemon-side scheduling controls for GPU/CPU/memory-sensitive media jobs,
  including per-backend and per-model concurrency caps when configured.
- Make cancellation, daemon shutdown, and garbage collection cooperate with
  active leases and retained job artifacts.

### Media Serving Wrappers And Runtime Stream Proxy

- Decide which media capabilities deserve long-lived `tentgent server` routes
  and which must remain durable daemon job workflows.
- Keep existing direct server capabilities `chat`, `embedding`, and `rerank`
  stable while evaluating media route expansion separately.
- Treat `vision-chat` and `audio-transcription` as candidate direct serving
  wrappers only after M7.
- Keep `audio-speech`, image generation/editing, video understanding, and video
  generation as durable job workflows unless a later plan proves a bounded
  direct route is safer and clearer.
- Decide whether media serving should live directly in the Python server, be
  mediated by the Rust daemon as a sidecar wrapper, or move to a separate
  serving runtime.
- Define request-scoped upload/temp-file cleanup, direct server upload limits,
  wrong-route errors, and health/readiness fields before adding media server
  routes.
- Avoid generic opaque runtime stream proxying. Prefer typed streaming contracts
  such as chat SSE or a future explicit speech streaming route.

### Conversion And Metadata Boundaries

- Decide whether Tentgent should ever offer explicit model conversion/import
  workflows that create missing runtime metadata.
- If conversion exists, make it a named user action with provenance and stored
  artifacts, not a hidden patch during runtime execution.
- Keep converted artifacts distinct from original pulled snapshots.

## Initialization Rule

This document is only a marker. After M7, initialize one focused plan at a time
from the work areas above. Each initialized plan should define concrete
contracts, write boundaries, test fixtures, and acceptance criteria before code
changes begin.
