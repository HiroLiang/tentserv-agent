# Post-M7 Platform Compatibility Roadmap

Status: archived. Current follow-up work is split between
[../v1.x-roadmap.md](../v1.x-roadmap.md) and
[../bugfix-maintenance-plan.md](../bugfix-maintenance-plan.md).

This roadmap starts the post-M7 architecture track. It replaces the former
future marker named `post-m7-runtime-compatibility-architecture.md` and broadens
the scope from runtime compatibility alone to the platform work needed for
reliable local deployment.

The completed M2-M7 release roadmap is archived in
[capability-first-release-roadmap.md](./capability-first-release-roadmap.md).

## Purpose

M6 and M7 made Tentgent useful across CLI, daemon REST, media jobs, MLX paths,
and signed macOS distribution. That release still leaves a larger product
problem: Tentgent can declare capabilities and route to runtime adapters, but it
does not yet have a durable compatibility system that proves which model,
adapter, backend, platform, and input shape actually works.

Post-M7 work should make compatibility explainable, durable, and inspectable
without hiding backend-specific conversion, metadata, or platform trust issues.

## Direction

- Keep the product surface CLI plus daemon REST.
- Treat static model and adapter metadata as hints, not runtime proof.
- Record successful and failed runtime attempts as durable compatibility
  evidence.
- Keep Apple Silicon and signed macOS/Homebrew distribution as first-class local
  deployment targets.
- Prefer typed workflow contracts over generic byte tunnels.
- Keep large content in filesystem stores; add indexed metadata only where it
  improves compatibility, proof, or query behavior.
- Do not silently synthesize missing model metadata to make a runtime smoke pass.
  If Tentgent ever creates metadata or converted artifacts, make it an explicit
  user action with provenance.

## Current Known Issues

- Signed Homebrew `0.4.1` can validate the Hugging Face Keychain entry through
  `tentgent auth hf`, but macOS still prompts for a password instead of biometric
  authentication in the observed flow. Record this as a platform trust and
  Keychain access-control investigation; do not block the next roadmap on an
  immediate fix.
- Model file layout is not enough to prove capability or runtime compatibility.
- Hugging Face, MLX, Diffusers, and Transformers model packages can be missing
  runtime-required files such as `preprocessor_config.json`, `tokenizer.json`,
  or `generation_config.json`.
- Model and adapter pull/import metadata are declarations or hints, not proof
  that a capability, backend, or adapter binding can run.
- Runtime errors currently disappear as one failed command instead of becoming
  durable compatibility evidence.
- A LoRA adapter verified with one model/backend/capability tuple must not be
  treated as verified with another tuple unless that exact tuple has its own
  proof.
- A filesystem-only catalog becomes awkward once Tentgent needs durable
  verified, failed, warning, and stale evidence across models, adapters, jobs,
  backend versions, and platform trust state.
- Some long-lived media servers may need explicit wrapping rather than direct
  Python route exposure.
- Runtime stream proxying needs typed route contracts. A generic byte tunnel
  would leak backend-specific protocols and blur Tentgent's stable API
  boundary.

## Work Tracks

### P1: Platform Trust And Distribution

- Audit signed Homebrew install behavior for `tentgent auth hf` and other
  Keychain-backed flows.
- Determine whether password-only prompts are caused by Keychain item access
  control, code requirement identity, Homebrew install path changes, command-line
  tool UI policy, or previous item creation state.
- Decide whether Tentgent should offer a keychain reset/migrate command for
  items created before Developer ID signing stabilized.
- Keep Developer ID signing and notarization verification documented as
  `codesign --verify --strict` plus Team ID/authority inspection for CLI
  binaries.
- Keep Homebrew formula updates and release artifact checksums repeatable.

### P2: Compatibility Proof Store

- Add a durable metadata/proof layer, likely SQLite-backed, beside the existing
  filesystem stores.
- Keep model snapshots, adapter snapshots, datasets, and job artifacts in the
  filesystem.
- Store proof records for model capability, runtime family, runtime package
  version, platform/device class, selected adapter, input/output shape, and
  structured success/failure state.
- Mark proof records stale when runtime packages, model metadata, adapter
  metadata, platform schema, or compatibility rules change.
- Make proof data inspectable from CLI, daemon DTOs, doctor output, and runtime
  error messages.

### P3: Model And LoRA Compatibility Management

- Separate declared capability, inferred capability, bound adapter intent,
  verified runtime proof, failed proof, and stale proof.
- Key model proof by at least `model_ref`, capability, backend/runtime family,
  runtime package version, platform/device class, and relevant input/output
  shape.
- Key LoRA proof by at least `adapter_ref`, `model_ref`, target capability,
  backend/runtime family, selected adapter weight file, runtime package version,
  and scale/config values when they affect loading.
- Require explicit re-verification before trusting a LoRA adapter on a different
  model, capability, backend, or selected weight file.
- Surface compatibility state in model/adapter inspect, list, server start
  checks, and job/runtime failures.

### P4: Dynamic Runtime Routing

- Add an explainable routing layer between stored metadata and concrete runtime
  adapters.
- Let Tentgent evaluate bounded candidate drivers when a model may be compatible
  with multiple runtime families.
- Preserve user-facing workflow contracts such as `transcribe`, `vision chat`,
  image generation, speech jobs, and video jobs.
- Avoid unbounded trial-and-error. Candidate attempts must be documented,
  cached, bounded, and easy to inspect.

### P5: Media Serving Wrappers And Stream Contracts

- Keep direct server capabilities `chat`, `embedding`, `rerank`, audio, vision,
  video, and image stable while continuing to route upload-heavy daemon APIs
  through durable jobs where needed.
- Keep direct server media routes path-based and model-bound; daemon-owned
  upload limits, temp-file cleanup, and generated-file serving stay in daemon
  job/workspace routes.
- Keep `audio-speech`, image generation/editing, video understanding, and video
  generation as durable job workflows unless a later plan proves a bounded
  direct route is safer and clearer.
- Avoid generic opaque runtime stream proxying. Prefer typed streaming contracts
  such as chat SSE or a future explicit speech streaming route.

### P6: Resource Coordination

- Add model/resource leases only where needed: model mutation, adapter writes,
  conversion, exclusive cache updates, server warm-model ownership, or backends
  that cannot safely share runtime state.
- Keep read-only inference concurrent by default unless a backend or operator
  setting defines stricter capacity limits.
- Add daemon-side scheduling controls for GPU/CPU/memory-sensitive media jobs,
  including per-backend and per-model concurrency caps when configured.
- Make cancellation, daemon shutdown, and garbage collection cooperate with
  active leases and retained job artifacts.

### P7: Shared Compatibility Registry Decision

- Decide whether Tentgent should publish or consume a shared compatibility
  registry for model/runtime/capability records.
- Keep local operation authoritative. Shared records can provide hints, not
  replace local probe results.
- Define trust, versioning, invalidation, opt-in behavior, and privacy
  boundaries before implementing online sharing.
- Distinguish community hints from verification on this machine.

### P8: Conversion And Metadata Boundaries

- Decide whether Tentgent should offer explicit model conversion/import
  workflows that create missing runtime metadata.
- If conversion exists, make it a named user action with provenance and stored
  artifacts, not a hidden patch during runtime execution.
- Keep converted artifacts distinct from original pulled snapshots.
- Ensure converted artifacts participate in the same compatibility proof model
  as pulled or imported models.

## Execution Order

1. Choose the first focused post-M7 slice from this roadmap and split it into a
   concrete execution plan.
2. Define the state model and proof vocabulary before changing runtime routing.
3. Decide whether the first slice should start with platform trust/Keychain
   behavior, SQLite proof foundations, or model/adapter compatibility records.
4. Keep each slice small enough to validate with targeted fixtures and explicit
   user-facing documentation.

## Acceptance For This Roadmap

- The previous M2-M7 roadmap is archived and no longer the active plan.
- Post-M7 work has one active router document with a name broad enough to cover
  platform trust, compatibility proof, runtime routing, model/adapter
  management, media serving, and resource coordination.
- Known post-release issues, including the macOS Keychain password-vs-biometric
  prompt behavior, are tracked without blocking the signed `v0.4.1` release.
