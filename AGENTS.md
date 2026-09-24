# AGENTS.md

This file is the shared project entry point for repository-level context.

Use this document to understand what the project contains, where important documentation lives, how Markdown files should be written, and which local `AGENTS.md` files may exist later in child directories.

If the current task is about agent workflows, role selection, or role-specific write boundaries, continue with [CLAUDE.md](./CLAUDE.md).

## Scope

- Define shared project context that applies across agents and subprojects.
- Act as the top-level index for documentation, runtime boundaries, and future local agent guides.
- Point contributors to the correct directories before task-specific work begins.

## Relationship To `CLAUDE.md`

- `AGENTS.md` explains the project map and shared repository rules.
- `CLAUDE.md` explains how agents should read, reason, and operate by role.
- Read `AGENTS.md` first to find context.
- Read `CLAUDE.md` next when the task needs an agent workflow or role.

## Repository Map

- `src/tentgent-kernel/`
  Shared Rust domain, infrastructure ports, runtime layout, machine capability
  state, and feature use cases.
- `src/tentgent-platform-fs/`
  Small safe Rust boundary around platform filesystem replacement primitives
  that require operating-system FFI outside the unsafe-free kernel crate.
- `src/tentgent-cli/`
  Rust CLI entry point.
- `src/tentgent-daemon/`
  Rust long-running daemon application host for bootstrap, transports,
  daemon-local runtime systems, and kernel use-case wiring.
- `python/tentgent-model-runtime/`
  Python model runtime subproject for direct HTTP execution workers, loaded
  model resources, backend implementations, task lifecycle, LoRA tuning, and
  Hugging Face snapshot helper integration.
- `python/tentgent-model-runtime/src/tentgent/runtime/`
  Importable Python package for model runtime server, task manager, backend
  resource manager, and concrete model implementations.
- `docs/contracts/`
  Concise contract documents for cross-module and cross-language interfaces.
- `docs/user/`
  User-facing install, upgrade, version, readiness, command, runtime, model fixture, and Keychain documentation.
- `docs/plans/`
  Active roadmap for larger runtime, backend, and release initiatives.
- `docs/development/`
  Developer-focused repository-local commands and testing notes.
- `docs/i18n/`
  Localized Markdown that mirrors English source documents.
- `docs/plans/archive/`
  Completed or superseded plans kept only for historical context and implementation history.

Current product surface:

- User-facing local work starts at the `tentgent` CLI.
- Long-running local HTTP workflows run through `tentgent daemon` and
  `src/tentgent-daemon/`.
- The former terminal UI and legacy Rust `core` / `http` crates have been
  removed. Archive docs may still mention them as history only.

Key current documents:

- [docs/contracts/runtime-home.md](./docs/contracts/runtime-home.md)
  Runtime-home resolution, environment-variable overrides, and standard storage roots.
- [docs/contracts/kernel-architecture.md](./docs/contracts/kernel-architecture.md)
  `tentgent-kernel` module placement, dependency direction, capability readiness, and persistence boundaries.
- [docs/contracts/auth-secrets.md](./docs/contracts/auth-secrets.md)
  Provider-secret resolution order and keychain usage rules.
- [docs/contracts/model-store.md](./docs/contracts/model-store.md)
  Model-store identity, deduplication, layout, and Hugging Face pull boundaries.
- [docs/contracts/resource-blockers.md](./docs/contracts/resource-blockers.md)
  Cross-process transition coordination and structured resource-mutation
  blocker rules.
- [docs/contracts/runtime-ownership.md](./docs/contracts/runtime-ownership.md)
  Durable route claims, physical runtime generations, guarded shutdown, and
  stale-state reconciliation.
- [docs/contracts/cluster.md](./docs/contracts/cluster.md)
  Cluster definition identity, canonical TOML storage, route-target validation,
  readiness diagnostics, local cluster server routing, daemon REST integration,
  guarded hot reload, runtime ownership, and resource protection.
- [docs/contracts/model-support-status.md](./docs/contracts/model-support-status.md)
  Support status vocabulary, evidence precedence, stale-proof rules, and
  transition rules for model/capability/backend tuples.
- [docs/contracts/model-support-proof-schema.md](./docs/contracts/model-support-proof-schema.md)
  Local proof and support hint record schema for explaining model support
  status.
- [docs/contracts/adapter-store.md](./docs/contracts/adapter-store.md)
  Adapter-store identity, compatibility metadata, layout, and source-index draft.
- [docs/contracts/dataset-store.md](./docs/contracts/dataset-store.md)
  Dataset-store identity, layout, local import, and deduplication boundary for training data.
- [docs/contracts/dataset-schema.md](./docs/contracts/dataset-schema.md)
  Canonical chat, tool-call, and cloud-generated dataset record schema.
- [docs/contracts/session-store.md](./docs/contracts/session-store.md)
  Local session metadata and transcript message store boundary.
- [docs/contracts/job-workspace.md](./docs/contracts/job-workspace.md)
  Kernel-owned job workspace, chunk IO, result file, and cleanup port boundary.
- [docs/contracts/server-chat.md](./docs/contracts/server-chat.md)
  HTTP chat request shape, adapter validation rules, and runtime error mapping.
- [docs/contracts/model-runtime-server.md](./docs/contracts/model-runtime-server.md)
  Direct Python model runtime health and graceful shutdown boundary.
- [docs/contracts/server-embedding.md](./docs/contracts/server-embedding.md)
  Direct local model-server embedding request shape and capability routing.
- [docs/contracts/server-rerank.md](./docs/contracts/server-rerank.md)
  Direct local model-server rerank request shape and capability routing.
- [docs/contracts/server-runtime-profile.md](./docs/contracts/server-runtime-profile.md)
  Runtime profile selection, storage, identity, launch, and inspect visibility
  for model-bound server starts.
- [docs/contracts/http-daemon.md](./docs/contracts/http-daemon.md)
  Rust HTTP daemon health/status endpoint, JSON response, and error-shape contract.
- [docs/contracts/provider-api-errors.md](./docs/contracts/provider-api-errors.md)
  Provider-shaped API unsupported-field, content, operation, and capability
  error semantics.
- [docs/contracts/tentgent-daemon.md](./docs/contracts/tentgent-daemon.md)
  Rust daemon application host, bootstrap, transport, and runtime-state boundary.
- [docs/contracts/training-lora.md](./docs/contracts/training-lora.md)
  Managed LoRA train-plan identity, config shape, backend rules, and run boundaries.
- [docs/user/README.md](./docs/user/README.md)
  Router for user-facing install, upgrade, command, version, and runtime documentation.
- [docs/user/install.md](./docs/user/install.md)
  Install, upgrade, pinned-version, PATH, and local package smoke-test guidance.
- [docs/user/version.md](./docs/user/version.md)
  User-facing release notes, stable promises, known limits, and upgrade
  expectations.
- [docs/user/1.0-readiness.md](./docs/user/1.0-readiness.md)
  User and contributor checklist for the `1.0.0` stability promise, release
  smoke expectations, and post-1.0 boundaries.
- [docs/user/commands.md](./docs/user/commands.md)
  Command index linking to focused feature examples, parameters, and HTTP formats.
- [docs/user/inference/README.md](./docs/user/inference/README.md)
  Chat, embedding, rerank, audio, vision, video, and image workflow guides.
- [docs/user/providers/README.md](./docs/user/providers/README.md)
  Provider-compatible curl and SDK examples, base URLs, and limitations.
- [docs/user/api.md](./docs/user/api.md)
  HTTP conventions and route-family index into feature-owned request/response docs.
- [docs/user/datasets.md](./docs/user/datasets.md)
  Dataset synthesis, evaluation, local commands, parameters, and HTTP fields.
- [docs/user/training-lora.md](./docs/user/training-lora.md)
  LoRA plan/run instructions, CLI-to-JSON mapping, and adapter selection.
- [docs/user/clusters.md](./docs/user/clusters.md)
  Plain-language Cluster definition, readiness, server routing, adapter,
  hot-reload, blocker, and recovery guidance.
- [docs/user/model-fixtures.md](./docs/user/model-fixtures.md)
  Recommended small model fixtures and smoke-test commands for chat, embedding,
  rerank, and metadata-only M6 media workflows.
- [docs/user/model-support-catalog.md](./docs/user/model-support-catalog.md)
  Built-in model-family catalog, support hint levels, and how catalog evidence
  differs from local verification proof.
- [docs/user/runtime.md](./docs/user/runtime.md)
  Runtime-home, platform/backend, environment override, and Keychain prompt notes.
- [docs/development/README.md](./docs/development/README.md)
  Developer command reference for source-first builds and repository-local tests.
- [docs/plans/v1.2.0-local-compatibility-state-plan.md](./docs/plans/v1.2.0-local-compatibility-state-plan.md)
  Active `v1.2.0` execution plan for issues `#126`-`#130`, with `#131` as a
  blocking runtime-lifecycle prerequisite, covering complete local
  compatibility tuples, proof v2 persistence, tuple-aware model, Cluster, and
  LoRA gates, diagnostics, recovery, documentation, and closeout.
- [docs/plans/issue-131-model-idle-release-plan.md](./docs/plans/issue-131-model-idle-release-plan.md)
  Implemented and validated issue-level contract, decision register,
  implementation evidence, and live smoke runbook for restoring model idle
  release and separating it from shared Python runtime process keep-alive.
- [docs/plans/v1.x-roadmap.md](./docs/plans/v1.x-roadmap.md)
  Active post-`v1.0.0` product roadmap. The `v1.1.0` Cluster MVP is complete;
  the selected `v1.2.0` compatibility-state slice has its own active execution
  plan, and later 1.x capabilities remain routed here.
- [docs/plans/bugfix-maintenance-plan.md](./docs/plans/bugfix-maintenance-plan.md)
  Active post-`v1.0.0` maintenance plan for bug fixes, diagnostics polish,
  release follow-up, documentation cleanup, and repository hygiene.
- [docs/plans/archive/README.md](./docs/plans/archive/README.md)
  Router for completed or superseded plans that should be consulted only when historical implementation context is needed.
- [docs/plans/archive/cluster-roadmap.md](./docs/plans/archive/cluster-roadmap.md)
  Completed `v1.1.0` Cluster MVP roadmap for issues `#113`-`#118`.
- [docs/plans/archive/cluster-runtime-ownership-plan.md](./docs/plans/archive/cluster-runtime-ownership-plan.md)
  Archived `#118` ownership, guard, and shutdown decision register.
- [docs/plans/archive/cluster-runtime-coordination-architecture.md](./docs/plans/archive/cluster-runtime-coordination-architecture.md)
  Implemented reusable coordination, ownership, supervisor, and watcher
  architecture.
- [docs/plans/archive/cluster-runtime-ownership-remediation.md](./docs/plans/archive/cluster-runtime-ownership-remediation.md)
  Closed `R1`-`R20` findings and final verification evidence for `#118`.
- [docs/plans/archive/v1.0.0-stable-compatibility-plan.md](./docs/plans/archive/v1.0.0-stable-compatibility-plan.md)
  Archived `v1.0.0` stable compatibility release train and post-merge release
  and Homebrew tap checklist.
- [docs/plans/archive/post-m7-platform-compatibility-roadmap.md](./docs/plans/archive/post-m7-platform-compatibility-roadmap.md)
  Archived post-M7 platform compatibility roadmap. Current follow-up work is
  split between the active `v1.x` roadmap and bugfix maintenance plan.
- [docs/plans/archive/post-1.0-serving-targets-and-multimodal-context-pipeline.md](./docs/plans/archive/post-1.0-serving-targets-and-multimodal-context-pipeline.md)
  Archived post-1.0 serving-target and multimodal-context planning note.
- [docs/plans/archive/provider-api-compatibility-and-model-support-roadmap.md](./docs/plans/archive/provider-api-compatibility-and-model-support-roadmap.md)
  Archived provider compatibility, model support, runtime profile, and 1.0
  readiness roadmap.
- [docs/plans/archive/v0.9.0-hardening-plan.md](./docs/plans/archive/v0.9.0-hardening-plan.md)
  Completed `v0.9.0` execution plan for 1.0 hardening, stable/experimental API
  audit, conformance smoke coverage, runtime recovery, cleanup, support proof
  retry behavior, readiness docs, and release closeout.
- [docs/plans/archive/capability-first-release-roadmap.md](./docs/plans/archive/capability-first-release-roadmap.md)
  Completed M2-M7 roadmap for model capability classification, embedding,
  rerank, media workflows, MLX backend parity, and Apple Developer ID signing.
- [docs/plans/archive/http-daemon-mvp.md](./docs/plans/archive/http-daemon-mvp.md)
  Completed service-entry plan for exposing Tentgent as a local HTTP daemon/API subsystem.
- [docs/plans/archive/cloud-provider-server-mvp.md](./docs/plans/archive/cloud-provider-server-mvp.md)
  Completed OpenAI and Claude cloud provider server routing plan.
- [docs/plans/archive/http-chat-streaming-mvp.md](./docs/plans/archive/http-chat-streaming-mvp.md)
  Completed Server-Sent Events streaming plan for local base-model, local adapter, and cloud provider chat.

## Project Naming

- Product slug: `tentgent`
- Binary name: `tentgent`
- Service host: `agent.tentserv.com`
- App identifier: `com.tentserv.tentgent`
- Environment variable prefix: `TENTGENT_`

## Kernel Boundary Rule

- `src/tentgent-kernel/` should avoid CLI, HTTP, daemon, or other
  entrypoint-specific naming unless the type is explicitly an adapter boundary
  for that entrypoint.
- Kernel domain, port, and use-case names should describe product capabilities,
  intents, state, and infrastructure roles rather than the caller that first
  used them.
- If an entrypoint needs special behavior, keep that mapping in the entrypoint
  crate or adapter layer and pass kernel-owned intent data across the boundary.

## Rust Module Structure Rule

- Treat `mod.rs` and `lib.rs` as composition files. They should declare modules,
  re-export public surface, and carry package-level documentation only.
- Do not place feature logic, infrastructure implementations, or large test
  bodies in `mod.rs` or `lib.rs`.
- Follow the nearest existing package shape before adding a new file. Prefer
  focused files such as `resolver.rs`, `store.rs`, `probe.rs`, `planner.rs`, or
  `executor.rs` over a single broad implementation file.
- Keep structure consistent across kernel packages: scan broadly for the local
  pattern, then make the smallest scoped change.

## Documentation Routing Rules

- The root `AGENTS.md` is the global index for repository-wide context.
- Future subprojects may add their own local `AGENTS.md` files for subtree-specific rules.
- When both the root and a child `AGENTS.md` exist, the nearest file to the working directory should win for local rules.
- The root `AGENTS.md` should still remain the top-level directory and documentation index.
- Use folder-level `README.md` files as routing documents when a subtree grows beyond what this file should summarize.

## Documentation Content Boundary

- Keep reusable feature commands, parameters, and API formats in versioned
  user docs, linked from the root README and `docs/user/README.md`.
- Keep personal interview scripts, machine-specific refs, generated datasets,
  and rehearsal results under the already ignored `test-data/` directory.
- Formal docs must not depend on ignored local notes. Use placeholders for
  machine-specific paths and refs in reusable examples.

## Documentation Update Rule

- If an approved change affects repository structure, requirements, runtime boundaries, entry points, or contracts, update the affected Markdown files in the same change.
- Apply `skill-creator` principles when updating Markdown: keep it concise, split by concern, avoid duplication, and prefer folder-plus-`README.md` expansion over growing one large document.
- Treat unnecessary documentation growth as a defect. Add structure only when it reduces reading cost for later agents.
- General source files may grow up to roughly 1000 lines before splitting becomes mandatory, though smaller focused modules are preferred.
- Human-readable Markdown may grow up to roughly 500 lines when it remains easy to scan.
- Agent-routing Markdown, skill files, and direction-setting Markdown should stay near 300 lines because they are read for orientation and token efficiency.

## Documentation Writing Rules

- `README.md` should be written primarily in English.
- Localized Markdown should live under `docs/i18n/`.
- All Markdown files outside `docs/i18n/` should be written in English.
- Link localized files from the corresponding English source document.
- The English version is the source of truth for localized counterparts.

## What This File Should Track

- Top-level project structure and documentation entry points.
- Paths to runtime, adapter, and contract documents.
- Shared naming and path conventions.
- Cross-module decisions, glossary terms, and shared context that multiple agents need.
- Links to future local `AGENTS.md` files owned by subprojects.

## Expansion Conventions

- If `docs/contracts/` grows, split it by interface or subsystem and keep this file as the top-level router only.
- If `docs/plans/` grows, keep unfinished work at the top level and move completed plans into `docs/plans/archive/`.
- If `python/tentgent-model-runtime/` grows, add a subtree `README.md` or `AGENTS.md` to route backend-specific reads.
- Keep the Python subproject in standard `pyproject.toml + src/` layout so IDE and packaging behavior remain predictable.
- If a `src/` subtree gains local rules, add a subtree `AGENTS.md` and link to it from this file.
