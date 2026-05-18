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

## Archived Plans

- [packaging-install-mvp.md](./packaging-install-mvp.md)
  Superseded release/install track. Kept for historical package, installer,
  Homebrew, and release automation details.
- [apple-signed-cli-release.md](./apple-signed-cli-release.md)
  Superseded standalone macOS signing plan. Current signing order is folded into
  the active capability-first release roadmap.
- [linux-release-support.md](./linux-release-support.md)
  Superseded Linux release/install track. Kept for prerelease Linux tarball,
  installer, and runtime smoke history.
- [tentgent-daemon-runtime.md](./tentgent-daemon-runtime.md)
  Superseded daemon runtime systems plan. Keep as background for future job and
  progress orchestration work.
- [model-capabilities-embedding-rerank.md](./model-capabilities-embedding-rerank.md)
  Superseded model capability plan. Current execution order is now in the active
  roadmap.
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
  Completed adapter, dataset, LoRA training, and request-time adapter execution milestone.
- [cloud-dataset-mvp.md](./cloud-dataset-mvp.md)
  Completed OpenAI/Claude-assisted dataset validation, prompt-template generation, synthesis, debugging, and evaluation.
- [cloud-provider-server-mvp.md](./cloud-provider-server-mvp.md)
  Completed OpenAI and Claude cloud provider server routing through local `tentgent server` chat.
- [http-chat-streaming-mvp.md](./http-chat-streaming-mvp.md)
  Completed Server-Sent Events streaming for local base-model, local adapter, and cloud provider chat.
- [tentgent-kernel-migration.md](./tentgent-kernel-migration.md)
  Completed kernel consolidation record. The active workspace now uses
  `tentgent-kernel` directly, with the legacy Rust core and HTTP crates removed.
