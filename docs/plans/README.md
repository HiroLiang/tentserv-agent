# Plans

Use this directory for the current active implementation roadmap and any
still-open plans that are too large or too cross-cutting to execute safely in
one pass without a staged breakdown.

## Scope

- Record step-by-step execution plans before large runtime, server, backend, or
  release changes.
- Keep one active roadmap unless a future initiative needs its own focused plan.
- Prefer short, action-oriented documents over long design essays.

## Routing Rule

- Keep each plan focused on one execution track.
- If a plan grows large, split it into subfolders with a local `README.md`.
- Update the plan when the approved execution order changes materially.
- Prefer review-sized implementation slices over one large execution step.
- When a track is being developed interactively, document the next small slice explicitly.
- Move completed or superseded plans into `archive/` so the top-level plan
  directory stays focused on current work.

## Active Plan Index

- [capability-first-release-roadmap.md](./capability-first-release-roadmap.md)
  Active roadmap after `v0.3.5-alpha.0`: model capability classification,
  embedding and rerank endpoint work, M6 media workflow slices, and Apple
  Developer ID signing before beta or release candidate tags.
- [m2-model-capability-detection-and-correction.md](./m2-model-capability-detection-and-correction.md)
  Detailed M2 slice: Hugging Face capability detection, manual metadata
  correction, and clear default-chat fallback warnings.
- [m3-server-compatibility-gates.md](./m3-server-compatibility-gates.md)
  Implemented M3 slice: server capability metadata, daemon server DTOs, and
  endpoint-family compatibility gates before embedding and rerank runtime work.
- [m4-embedding-mvp.md](./m4-embedding-mvp.md)
  Implemented M4 slice: native embedding endpoint, embedding runtime port,
  first backend path, and endpoint-family isolation from chat sessions.
- [m5-rerank-mvp.md](./m5-rerank-mvp.md)
  Implemented M5 slice: native rerank endpoint, rerank runtime port, first local
  cross-encoder backend path, CLI one-shot embedding/rerank helpers, and
  endpoint-family isolation from chat sessions.
- [m6a-multimodal-contracts.md](./m6a-multimodal-contracts.md)
  Implemented M6A slice: metadata-only multimodal capability vocabulary,
  transport shape decisions, opaque proxy boundary, and small Hugging Face smoke
  fixtures.
- [m6b-kernel-job-workspace-foundation.md](./m6b-kernel-job-workspace-foundation.md)
  M6B refactor slice: kernel-owned job workspace ports, chunk IO, result files,
  cleanup, and daemon runtime wiring before media model execution.
- [m6c-audio-transcription-daemon-mvp.md](./m6c-audio-transcription-daemon-mvp.md)
  M6C implementation record for daemon audio transcription path jobs, Python
  runtime wiring, output formats, doctor guidance, and smoke-test evidence.
- [m6d-audio-transcription-file-stream-job-input.md](./m6d-audio-transcription-file-stream-job-input.md)
  Implemented M6D slice for the canonical
  `POST /v1/audio/transcriptions/job` file-stream job endpoint, result
  readiness semantics, and internal workspace persistence.
- [m6e-audio-transcription-cli-and-large-file-hardening.md](./m6e-audio-transcription-cli-and-large-file-hardening.md)
  Implemented M6E slice for foreground `tentgent transcribe`, output
  behavior, large-file guardrails, and audio CLI documentation.
- [m6f-vision-chat-image-input.md](./m6f-vision-chat-image-input.md)
  Implemented M6F slice for native single-image `vision-chat`, foreground CLI,
  multipart daemon endpoint, and dedicated vision runtime contracts.
- [m6g-image-generation-jobs.md](./m6g-image-generation-jobs.md)
  Implemented M6G native text-to-image artifact jobs, Diffusers runtime
  support, foreground CLI output, and workflow-owned result file routes.
- [m6h-mlx-multimodal-backend-foundation.md](./m6h-mlx-multimodal-backend-foundation.md)
  Implemented M6H foundation for Apple Silicon media backend parity: MLX
  runtime family metadata and routing guardrails across vision, audio, and
  image workflows without replacing the implemented safetensors/Diffusers
  runtime paths.
- [m6i-mlx-vision-chat-backend.md](./m6i-mlx-vision-chat-backend.md)
  Implemented M6I backend path for making the existing native `vision-chat`
  CLI and daemon endpoint route to `mlx-vlm` models on Apple Silicon, with CLI
  and daemon smoke evidence recorded.
- [m6j-mlx-audio-runtime-backend.md](./m6j-mlx-audio-runtime-backend.md)
  Implemented M6J backend path for making the existing native
  `audio-transcription` CLI and daemon job route to `mlx-audio` models on
  Apple Silicon without adding new user-facing audio APIs, with CLI and daemon
  smoke evidence recorded.
- [m6k-mlx-image-generation-backend-decision.md](./m6k-mlx-image-generation-backend-decision.md)
  Implemented M6K decision/backend slice for routing Apple Silicon
  `mlx-diffusion` image-generation models through MFLUX behind the existing
  `tentgent image generate` and daemon image job surfaces.

## Recommended Order

1. Continue the remaining M6 media workflow slices from the active roadmap.
2. Keep full model compatibility architecture in the post-M7 marker plan until
   a focused future execution plan is initialized.
3. Continue each later workflow only after its API/output contract and backend
   family decision are stable.
4. Run M7 Apple Developer ID signing and notarization on prerelease artifacts
   before beta or release candidate tags.

## Future Plan Markers

- [post-m7-runtime-compatibility-architecture.md](./post-m7-runtime-compatibility-architecture.md)
  Future marker only, not initialized for execution: model compatibility
  probe/cache, dynamic runtime transduction, optional shared compatibility
  registry, model resource coordination, and conversion boundaries after the
  current M6-to-M7 release track.

## Deferred Plans

- No terminal UI redesign track is active. The product surface is CLI plus
  daemon REST.

## Archived Plans

- [archive/README.md](./archive/README.md)
  Router for completed or superseded plans that are kept only for historical
  context.
