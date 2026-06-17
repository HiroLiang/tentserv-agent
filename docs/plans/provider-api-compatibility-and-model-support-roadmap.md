# Provider API Compatibility, Model Support, And 1.0 Readiness Roadmap

Status: active focused roadmap.

This roadmap turns the post-M7 compatibility direction into a product-facing
track: make Tentgent easier to adopt as a local or hosted-compatible runtime by
supporting familiar provider API shapes and by making model support explicit,
inspectable, and bounded. It also defines the staged path from the current
0.x line toward a trustworthy `1.0.0` release.

## Relationship To Post-M7 Roadmap

[post-m7-platform-compatibility-roadmap.md](./post-m7-platform-compatibility-roadmap.md)
is the broad architecture roadmap for platform trust, durable compatibility
proofs, runtime routing, resource coordination, and conversion boundaries.

This document is the focused execution roadmap for:

- OpenAI, Claude/Anthropic, and Gemini-compatible HTTP surfaces.
- Model support records and operator-facing support status.
- Runtime parameter profiles for model, backend, quantization, and platform
  differences.
- Request routing that explains why a model is accepted, rejected, or treated as
  unknown.
- The minimum compatibility, diagnostic, and documentation work needed before
  `1.0.0`.

## Purpose

Tentgent should let users point existing OpenAI, Claude, or Gemini clients at a
Tentgent domain or local server when Tentgent supports the same capability.
That compatibility must be honest: supported endpoint shapes should work
predictably, and unsupported fields or endpoint families should fail with clear
stable errors.

Tentgent should also stop treating model metadata as enough proof. Operators
need to know whether a model is known supported, locally verified, failed,
unknown, or stale before they put it behind a server or job workflow.

## Product Direction

- Make compatible provider APIs a first-class product surface, not a thin
  best-effort proxy.
- Keep Tentgent-owned native APIs stable for local workflows that do not map
  cleanly to provider APIs.
- Publish a support matrix for compatible endpoint families and accepted
  request fields.
- Preserve local proof as the authority. Built-in or shared support records are
  hints until this machine verifies the model/backend/platform tuple.
- Prefer typed runtime profiles over scattered backend-specific conditionals.
- Reject unsupported models and request parameters early when Tentgent can prove
  they cannot work.
- Allow unknown models only through explicit policy, and record the result as
  compatibility evidence.
- Treat `1.0.0` as a stability promise, not a feature-count milestone.

## 1.0 Readiness Definition

Tentgent should reach `1.0.0` only when supported workflows can be trusted in
real use and unsupported workflows fail clearly.

Before `1.0.0`, Tentgent should provide:

- Stable native daemon contracts for chat, embeddings, rerank, and durable media
  job workflows.
- Stable compatible API contracts for the documented OpenAI, Claude/Anthropic,
  and Gemini endpoint subset.
- Stable error shapes and stable unsupported-field behavior.
- Inspectable model support status that distinguishes hints from local proof.
- Runtime parameter profiles for the first supported backend families.
- Installation, upgrade, runtime bootstrap, and doctor flows that give
  actionable recovery steps.
- Conformance tests for supported compatible endpoint shapes and curated model
  fixture smoke tests for core capabilities.
- Documentation that lets operators answer what is supported, what is
  experimental, and how to recover from common failures.

## Compatibility Promise

Tentgent should aim for domain-swap compatibility only within declared support:

- If an endpoint and field are marked supported, users should be able to change
  the base URL and keep the request shape.
- If a provider has an endpoint that Tentgent does not implement, return a clear
  unsupported error instead of silently degrading behavior.
- If a request includes unsupported fields such as tools, audio content, logprobs,
  or provider-specific response formats, reject them before runtime dispatch
  unless a deliberate adapter exists.
- Response shapes should be provider-compatible where the compatible endpoint is
  used, and Tentgent-native where the native endpoint is used.

## Post-1.0 Direction

The path to `1.0.0` should leave room for future serving targets that group
multiple capability-specific local models behind one API target. Automatic
media pre-processing, where images, audio, video, or files are parsed by the
configured capability model before their extracted context is sent to a chat
model, is tracked separately in
[post-1.0-serving-targets-and-multimodal-context-pipeline.md](./post-1.0-serving-targets-and-multimodal-context-pipeline.md).

That future pipeline is not a `1.0.0` blocker. Before `1.0.0`, the important
constraint is to keep provider request parsing, native intent types, capability
metadata, runtime profiles, and attachment handling explicit enough that this
post-1.0 direction remains possible.

## Work Tracks

### P1: Provider API Compatibility Matrix

- Define a versioned compatibility matrix for OpenAI, Claude/Anthropic, and
  Gemini endpoint families.
- Track endpoint support at least for chat, streaming chat, embeddings, image
  generation, audio transcription, audio speech, vision chat, and future video
  workflows.
- Track field-level support for common request options such as model, messages,
  system prompt, temperature, max tokens, stream, tools, response format,
  dimensions, image size, voice, language, and output format.
- Keep matrix entries in user-facing docs and reference them from contract docs
  once implementation begins.
- Use stable error codes for unsupported endpoint families and unsupported
  fields.

### P2: Normalized Provider Request Boundary

- Normalize provider-compatible requests into Tentgent-owned intent types before
  touching runtime adapters.
- Keep provider parsing, validation, and response rendering outside HTTP
  handlers so they can be tested without a live server.
- Preserve provider-specific response shapes at compatible endpoints.
- Keep native endpoints free to expose Tentgent-specific fields such as
  `model_ref`, `adapter_ref`, job ids, and local result-file routes.
- Add conformance fixtures for supported provider request and response shapes.

### P3: Model Support Registry

- Use [model-support-status.md](../contracts/model-support-status.md) as the
  vocabulary and precedence contract for support status resolution.
- Use
  [model-support-proof-schema.md](../contracts/model-support-proof-schema.md)
  as the local proof and support hint record schema.
- Add a local support registry that can answer whether a model/backend/platform
  tuple is supported, unsupported, verified, failed, unknown, or stale.
- Store records keyed by model identity, source repo, source revision,
  capability, backend/runtime family, format, quantization, runtime package
  version, and platform/device class.
- Record constraints such as minimum memory, context limits, required files,
  unsupported parameters, known tokenizer requirements, and adapter support.
- Treat built-in support records as hints and local proof records as
  authoritative.
- Keep enough provenance to explain whether a status came from built-in rules,
  user override, local verification, failed runtime execution, or a future
  shared registry.

### P4: Runtime Parameter Profiles

- Introduce runtime profiles that map normalized requests into backend-specific
  parameters.
- Profile by capability, backend family, model format, quantization, model size,
  platform/device class, and known model family where needed.
- Track safe defaults and hard limits for context length, max output tokens,
  temperature/top-p support, dimensions, image sizes, voices, audio formats,
  LoRA scale, GPU layers, precision, and memory-sensitive knobs.
- Drop, translate, or reject parameters explicitly. Do not pass unknown
  provider fields blindly into backend calls.
- Make the selected profile visible in server start output, inspect output, and
  relevant runtime errors.

### P5: Verification And Gating

- Add a verification flow that can create proof records without requiring users
  to discover failures during production traffic.
- Gate server starts and daemon jobs using declared capability, support status,
  platform readiness, runtime profile availability, and stale proof state.
- Treat support status as a derived result from the
  [model-support-status.md](../contracts/model-support-status.md) resolver
  rules, not as a direct replacement for declared `model_capabilities`.
- Provide clear override policy for unknown models, such as the explicit
  server `--allow-unverified` flag, and record the outcome in proof workflows.
- Make failures actionable: missing files, unsupported quantization, missing
  runtime package, memory pressure, unsupported parameter, adapter mismatch, or
  provider API mismatch should have distinct messages.
- Surface support state in model list, model inspect, server inspect, doctor,
  and daemon error responses.

### P6: Dynamic Runtime Routing

- Add routing only after support records and runtime profiles exist.
- Evaluate a bounded set of candidate backends when a model may run through
  multiple families, such as safetensors through Transformers or MLX, or GGUF
  through llama-cpp.
- Prefer verified routes, then supported hinted routes, then explicit user
  override routes.
- Cache attempted routes as proof records so runtime failures improve future
  decisions.
- Avoid unbounded trial-and-error; every candidate should have an explainable
  reason.

### P7: Documentation And Operator UX

- Add a concise compatibility matrix for supported provider-compatible
  endpoints.
- Add model support documentation that explains supported, verified, failed,
  unknown, and stale states.
- Keep curl examples for both native Tentgent APIs and provider-compatible
  APIs.
- Make command output compact but explainable, with details available through
  inspect commands.

### P8: 1.0 Hardening

- Freeze stable native and provider-compatible API surfaces before cutting
  `1.0.0`.
- Define migration expectations for runtime-home metadata, model stores,
  adapter stores, datasets, sessions, server specs, and proof records.
- Harden install and upgrade recovery paths across direct installers,
  package-manager installs, and managed Python runtime bootstrap.
- Expand `tentgent doctor` so common auth, runtime, package, platform, and
  model-support failures point to a next action.
- Add cancellation, cleanup, and shutdown behavior for long-running jobs and
  daemon-managed runtime work where missing behavior would surprise operators.

## Execution Flow

1. Inventory the current compatible endpoints and field behavior.
2. Document the first compatibility matrix before expanding behavior.
3. Refactor provider-compatible request parsing into testable adapters where
   needed.
4. Add conformance tests for already-supported endpoint shapes.
5. Define the support/proof vocabulary and storage boundary.
6. Add model support records for a small set of known fixtures.
7. Add runtime profiles for the first high-value backend families.
8. Wire server start and daemon job gates to support status.
9. Add bounded dynamic routing after verified support records exist.
10. Harden install, upgrade, doctor, cancellation, and cleanup paths.
11. Freeze the documented stable API subset and publish `1.0.0` only after
    conformance and fixture tests pass.
12. Revisit shared/community registry only after local registry behavior is
    stable.

## Release Milestones

### v0.6.0: Compatibility Contract Release

- Inventory OpenAI, Claude/Anthropic, and Gemini endpoint coverage.
- Document supported, partial, planned, and unsupported endpoint families.
- Define stable unsupported-field and unsupported-endpoint errors.
- Add tests for current OpenAI-compatible chat, embeddings, and image generation
  behavior.
- Keep this release focused on what Tentgent supports and how unsupported
  requests fail.

### v0.7.0: Support Status Release

- Defined support states: `supported`, `verified`, `failed`, `unknown`,
  `unsupported`, and `stale`.
- Defined tuple-aware proof keys and support hint records for model capability,
  backend, runtime, platform, and evidence provenance.
- Added a built-in model support catalog for fixtures and major model families.
- Surfaced support state in `model ls`, `model inspect`, `server inspect`, and
  `doctor` before enforcing stricter gates.
- Kept hard support-status gating, automatic request-time proof updates, and
  runtime profile selection for the `v0.8.0` runtime profile and gating track.

### v0.8.0: Runtime Profile And Gating Release

- Add profile planning for the first local chat and embedding backend families.
- Record accepted parameters, rejected parameters, default context/output
  limits, and backend-specific knobs.
- Surface the selected profile in inspect output or runtime diagnostics.
- Gate direct server start with declared capability, support status, and runtime
  profile availability.
- Keep unknown models usable only through explicit policy.
- Record successful and failed launches as support evidence.

### v0.9.0: 1.0 Hardening Release

- Expand provider conformance tests and curated model fixture smoke tests.
- Harden install, upgrade, runtime bootstrap, doctor, and auth recovery flows.
- Add missing cancellation, cleanup, and shutdown behavior that blocks stable
  operational use.
- Review native and compatible API contracts for fields that must be frozen,
  renamed, or clearly marked experimental before `1.0.0`.
- Prepare user-facing stable/experimental documentation and upgrade notes.

### v1.0.0: Stable Compatibility Release

- Freeze the documented native and provider-compatible API subset.
- Keep unsupported endpoint families and unsupported fields on stable clear
  errors.
- Require model support status, runtime profile selection, and proof state to be
  inspectable for stable workflows.
- Keep advanced dynamic routing, shared registries, broader media realtime
  support, and additional provider fields available for later `1.x` releases.

## Out Of Scope For This Roadmap

- A public shared compatibility registry service.
- Automatic hidden model conversion or metadata synthesis.
- Full provider API parity for every field and endpoint.
- Generic byte-tunnel proxying to provider or backend protocols.
- Replacing Tentgent-native job APIs where provider APIs cannot express local
  file ownership, cleanup, or artifact serving clearly.

## Acceptance Criteria

- Tentgent has a documented provider compatibility matrix.
- Supported compatible endpoints have request/response conformance tests.
- Unsupported provider fields and endpoint families fail with stable clear
  errors.
- Model support status is inspectable and distinguishes hints from local proof.
- Runtime parameter profiles exist for the first supported backend families.
- Server and job routing can explain why a model is accepted, rejected,
  unknown, or stale.
- `1.0.0` is cut only after the documented stable API subset, conformance
  tests, fixture tests, diagnostics, and upgrade expectations are ready.
