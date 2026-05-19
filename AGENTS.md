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
- `src/tentgent-cli/`
  Rust CLI entry point.
- `src/tentgent-daemon/`
  Rust long-running daemon application host for bootstrap, transports,
  daemon-local runtime systems, and kernel use-case wiring.
- `python/tentgent-daemon/`
  Standalone Python subproject that owns model runtimes, backend selection, and adapter lifecycle.
- `python/tentgent-daemon/src/tentgent_daemon/`
  Importable Python package for runtime, backend, CLI, and internal helper modules.
- `docs/contracts/`
  Concise contract documents for cross-module and cross-language interfaces.
- `docs/user/`
  User-facing install, upgrade, version, command, runtime, model fixture, and Keychain documentation.
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

- `docs/contracts/runtime-home.md`
  Runtime-home resolution, environment-variable overrides, and standard storage roots.
- `docs/contracts/kernel-architecture.md`
  `tentgent-kernel` module placement, dependency direction, capability readiness, and persistence boundaries.
- `docs/contracts/auth-secrets.md`
  Provider-secret resolution order and keychain usage rules.
- `docs/contracts/model-store.md`
  Model-store identity, deduplication, layout, and Hugging Face pull boundaries.
- `docs/contracts/adapter-store.md`
  Adapter-store identity, compatibility metadata, layout, and source-index draft.
- `docs/contracts/dataset-store.md`
  Dataset-store identity, layout, local import, and deduplication boundary for training data.
- `docs/contracts/dataset-schema.md`
  Canonical chat, tool-call, and cloud-generated dataset record schema.
- `docs/contracts/session-store.md`
  Local session metadata and transcript message store boundary.
- `docs/contracts/server-chat.md`
  HTTP chat request shape, adapter validation rules, and runtime error mapping.
- `docs/contracts/server-embedding.md`
  Direct Python server embedding request shape and capability routing.
- `docs/contracts/server-rerank.md`
  Direct Python server rerank request shape and capability routing.
- `docs/contracts/http-daemon.md`
  Rust HTTP daemon health/status endpoint, JSON response, and error-shape contract.
- `docs/contracts/tentgent-daemon.md`
  Rust daemon application host, bootstrap, transport, and runtime-state boundary.
- `docs/contracts/training-lora.md`
  Managed LoRA train-plan identity, config shape, backend rules, and future run boundaries.
- `docs/user/README.md`
  Router for user-facing install, upgrade, command, version, and runtime documentation.
- `docs/user/install.md`
  Install, upgrade, pinned-version, PATH, and local package smoke-test guidance.
- `docs/user/version.md`
  Current MVP feature list, known limits, and upgrade expectations.
- `docs/user/commands.md`
  User command examples for auth, model, adapter, dataset, chat, server, and LoRA training flows.
- `docs/user/model-fixtures.md`
  Recommended small model fixtures and smoke-test commands for chat, embedding,
  rerank, and metadata-only M6 media workflows.
- `docs/user/runtime.md`
  Runtime-home, platform/backend, environment override, and Keychain prompt notes.
- `docs/development/README.md`
  Developer command reference for source-first builds and repository-local tests.
- `docs/plans/capability-first-release-roadmap.md`
  Active roadmap for model capability classification, embedding, rerank,
  metadata-only media capability vocabulary, M6B media jobs, and prerelease
  Apple Developer ID signing.
- `docs/plans/archive/README.md`
  Router for completed or superseded plans that should be consulted only when historical implementation context is needed.
- `docs/plans/archive/http-daemon-mvp.md`
  Completed service-entry plan for exposing Tentgent as a local HTTP daemon/API subsystem.
- `docs/plans/archive/cloud-provider-server-mvp.md`
  Completed OpenAI and Claude cloud provider server routing plan.
- `docs/plans/archive/http-chat-streaming-mvp.md`
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
- If `python/tentgent-daemon/` grows, add a subtree `README.md` or `AGENTS.md` to route backend-specific reads.
- Keep the Python subproject in standard `pyproject.toml + src/` layout so IDE and packaging behavior remain predictable.
- If a `src/` subtree gains local rules, add a subtree `AGENTS.md` and link to it from this file.
