# Plan Archive

Use this directory for completed or superseded plans that are no longer the
active execution track.

## Scope

- Keep historical plans available when implementation history matters.
- Remove completed or superseded plans from the active plan surface in the
  parent folder.
- Keep the active `docs/plans/` directory focused on current or future work.
- Archived plans preserve historical names and commands from their original
  implementation period, including removed crates or removed UI surfaces. Do
  not treat archive references as current product direction.

## Routing Rule

- Start in the parent [README.md](../README.md) for the active roadmap.
- Read this archive only when you need:
  - implementation history
  - completed slice order
  - rationale behind earlier runtime, release, server, or backend decisions

## v1.0.0 Compatibility Track

These plans were the staged compatibility and release-readiness path through
the `v1.0.0` release closeout.

- [v1.0.0-stable-compatibility-plan.md](./v1.0.0-stable-compatibility-plan.md)
  Completed release train for freezing stable API surfaces, proving native and
  provider-compatible workflows, validating install and diagnostics readiness,
  and preparing the post-merge release/tap checklist.
- [provider-api-compatibility-and-model-support-roadmap.md](./provider-api-compatibility-and-model-support-roadmap.md)
  Completed focused roadmap for provider-shaped API compatibility, model
  support status, runtime profiles, verification gates, and the path from the
  0.x line to the `v1.0.0` stability promise.

## v0.9.0 Hardening Track

These plans were the active 1.0 hardening roadmap through the `v0.9.0`
closeout issue and were archived after `#82` completed.

- [v0.9.0-hardening-plan.md](./v0.9.0-hardening-plan.md)
  Completed execution plan for stable/experimental API audit, conformance smoke
  coverage, runtime and doctor recovery, install and upgrade hardening,
  cancellation and cleanup semantics, support proof recovery, readiness docs,
  and release closeout.
- [v0.9.0-api-surface-audit-findings.md](./v0.9.0-api-surface-audit-findings.md)
  Archived API surface audit findings and follow-up routing record from issue
  `#75`.

## Capability-First M2-M7 Track

These plans were the active release roadmap through signed `v0.4.1` and were
archived after M7 completed.

- [capability-first-release-roadmap.md](./capability-first-release-roadmap.md)
  Completed roadmap for model capability classification, embedding, rerank, M6
  media workflows, MLX backend parity, and M7 Apple Developer ID release
  engineering.
- [m2-model-capability-detection-and-correction.md](./m2-model-capability-detection-and-correction.md)
  Completed M2 slice for Hugging Face capability detection, manual metadata
  correction, and default-chat fallback warnings.
- [m3-server-compatibility-gates.md](./m3-server-compatibility-gates.md)
  Completed M3 slice for server capability metadata, daemon server DTOs, and
  endpoint-family compatibility gates.
- [m4-embedding-mvp.md](./m4-embedding-mvp.md)
  Completed M4 slice for native embedding endpoint, runtime port, backend path,
  and endpoint-family isolation.
- [m5-rerank-mvp.md](./m5-rerank-mvp.md)
  Completed M5 slice for native rerank endpoint, runtime port, local
  cross-encoder backend path, and CLI smoke helpers.
- [m6a-multimodal-contracts.md](./m6a-multimodal-contracts.md)
  Completed M6A slice for multimodal capability vocabulary, transport shape
  decisions, opaque proxy boundary, and small HF fixtures.
- [m6b-kernel-job-workspace-foundation.md](./m6b-kernel-job-workspace-foundation.md)
  Completed M6B foundation for kernel-owned job workspace ports, chunk IO,
  result files, cleanup, and daemon runtime wiring.
- [m6c-audio-transcription-daemon-mvp.md](./m6c-audio-transcription-daemon-mvp.md)
  Completed M6C daemon audio transcription jobs and Python runtime wiring.
- [m6d-audio-transcription-file-stream-job-input.md](./m6d-audio-transcription-file-stream-job-input.md)
  Completed M6D file-stream transcription job endpoint and result readiness
  semantics.
- [m6e-audio-transcription-cli-and-large-file-hardening.md](./m6e-audio-transcription-cli-and-large-file-hardening.md)
  Completed M6E foreground transcription CLI and large-file guardrails.
- [m6f-vision-chat-image-input.md](./m6f-vision-chat-image-input.md)
  Completed M6F native single-image vision chat CLI and daemon endpoint.
- [m6g-image-generation-jobs.md](./m6g-image-generation-jobs.md)
  Completed M6G text-to-image artifact jobs and Diffusers runtime support.
- [m6h-mlx-multimodal-backend-foundation.md](./m6h-mlx-multimodal-backend-foundation.md)
  Completed M6H MLX media backend metadata and routing foundation.
- [m6i-mlx-vision-chat-backend.md](./m6i-mlx-vision-chat-backend.md)
  Completed M6I `mlx-vlm` vision chat backend path.
- [m6j-mlx-audio-runtime-backend.md](./m6j-mlx-audio-runtime-backend.md)
  Completed M6J `mlx-audio` transcription backend path.
- [m6k-mlx-image-generation-backend-decision.md](./m6k-mlx-image-generation-backend-decision.md)
  Completed M6K MFLUX image generation backend decision and routing.
- [m6l-image-generation-lora.md](./m6l-image-generation-lora.md)
  Completed M6L managed image-generation LoRA adapter support.
- [m6m-image-to-image.md](./m6m-image-to-image.md)
  Completed M6M one-input-image image-to-image transform jobs.
- [m6n-inpainting-and-masks.md](./m6n-inpainting-and-masks.md)
  Completed M6N masked image inpainting jobs.
- [m6o-reference-images-and-controlnet.md](./m6o-reference-images-and-controlnet.md)
  Completed M6O typed controlled image generation and ControlNet-style adapter
  support.
- [m6p-audio-speech-jobs.md](./m6p-audio-speech-jobs.md)
  Completed M6P text-to-WAV speech artifact jobs.
- [m6q-video-understanding-jobs.md](./m6q-video-understanding-jobs.md)
  Completed M6Q video-understanding artifact jobs.
- [m6r-video-generation-artifact-decision.md](./m6r-video-generation-artifact-decision.md)
  Completed M6R internal video-generation artifact contract decision.
- [m6s-media-serving-and-runtime-stream-proxy-decision.md](./m6s-media-serving-and-runtime-stream-proxy-decision.md)
  Deferred M6S decision record that moved media serving wrappers and runtime
  stream proxy design to the post-M7 roadmap.
- [m7-apple-developer-id-release-pipeline.md](./m7-apple-developer-id-release-pipeline.md)
  Completed M7 release-engineering slice for Developer ID signing,
  notarization, temporary CI keychain import, and signed `v0.4.1` release
  readiness.

## Earlier Archived Plans

- [packaging-install-mvp.md](./packaging-install-mvp.md)
  Superseded release/install track. Kept for historical package, installer,
  Homebrew, and release automation details.
- [apple-signed-cli-release.md](./apple-signed-cli-release.md)
  Superseded standalone macOS signing plan. The completed signing implementation
  is recorded in the archived M7 plan.
- [linux-release-support.md](./linux-release-support.md)
  Superseded Linux release/install track. Kept for prerelease Linux tarball,
  installer, and runtime smoke history.
- [tentgent-daemon-runtime.md](./tentgent-daemon-runtime.md)
  Superseded daemon runtime systems plan. Keep as background for future job and
  progress orchestration work.
- [model-capabilities-embedding-rerank.md](./model-capabilities-embedding-rerank.md)
  Superseded model capability plan. The completed replacement track is archived
  in the capability-first roadmap.
- [0.3-bugfix-rollup.md](./0.3-bugfix-rollup.md)
  Completed post-`v0.3.0-alpha.1` correctness rollup for session context,
  daemon/server boundaries, stale daemon diagnostics, prerelease safeguards,
  human-facing size display, and runtime footprint visibility.
- [http-daemon-mvp.md](./http-daemon-mvp.md)
  Completed service-entry track for making the local HTTP daemon a
  programmatic peer of the main CLI workflows.
- [runtime-chat-mvp.md](./runtime-chat-mvp.md)
  Completed foundation for one-shot chat and backend routing.
- [server-runtime-mvp.md](./server-runtime-mvp.md)
  Completed first server lifecycle and management surface.
- [lora-server-mvp.md](./lora-server-mvp.md)
  Completed adapter, dataset, LoRA training, and request-time adapter execution
  milestone.
- [cloud-dataset-mvp.md](./cloud-dataset-mvp.md)
  Completed OpenAI/Claude-assisted dataset validation, prompt-template
  generation, synthesis, debugging, and evaluation.
- [cloud-provider-server-mvp.md](./cloud-provider-server-mvp.md)
  Completed OpenAI and Claude cloud provider server routing through local
  `tentgent server` chat.
- [cloud-runtime-test-tracker.md](./cloud-runtime-test-tracker.md)
  Completed OpenAI cloud runtime validation tracker for direct cloud servers,
  daemon cloud routes, dataset synth/eval, and Keychain secret resolution.
- [http-chat-streaming-mvp.md](./http-chat-streaming-mvp.md)
  Completed Server-Sent Events streaming for local base-model, local adapter,
  and cloud provider chat.
- [tentgent-kernel-migration.md](./tentgent-kernel-migration.md)
  Completed kernel consolidation record. The active workspace now uses
  `tentgent-kernel` directly, with the legacy Rust core and HTTP crates removed.
