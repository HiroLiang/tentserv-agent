# Post-M7 Runtime Compatibility Architecture

Status: future marker, not initialized for execution.

This plan records architecture problems discovered during M6 media backend work.
It is intentionally separate from the current M6-to-M7 release track. Do not use
this plan to block M7 signing, notarization, or release pipeline work.

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
- Operators need optional runtime capacity controls, but read-only inference
  should remain concurrent by default.
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
