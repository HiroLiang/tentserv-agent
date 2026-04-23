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

- `src/tentgent-core/`
  Shared Rust core types, runtime-facing contracts, and routing logic.
- `src/tentgent-cli/`
  Rust CLI entry point.
- `src/tentgent-http/`
  Rust HTTP entry point.
- `python/tentgent-daemon/`
  Standalone Python subproject that owns model runtimes, backend selection, and adapter lifecycle.
- `python/tentgent-daemon/src/tentgent_daemon/`
  Importable Python package for runtime, backend, CLI, and internal helper modules.
- `docs/contracts/`
  Concise contract documents for cross-module and cross-language interfaces.
- `docs/plans/`
  Execution plans for larger runtime and backend initiatives.
- `docs/i18n/`
  Localized Markdown that mirrors English source documents.

Key current documents:

- `docs/contracts/runtime-home.md`
  Runtime-home resolution, environment-variable overrides, and standard storage roots.
- `docs/contracts/auth-secrets.md`
  Provider-secret resolution order and keychain usage rules.
- `docs/contracts/model-store.md`
  Model-store identity, deduplication, layout, and Hugging Face pull boundaries.
- `docs/plans/runtime-chat-mvp.md`
  Staged plan for Python-first chat runtimes, backend routing, and future server layering.

## Project Naming

- Product slug: `tentgent`
- Binary name: `tentgent`
- Service host: `agent.tentserv.com`
- App identifier: `com.tentserv.tentgent`
- Environment variable prefix: `TENTGENT_`

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
- Treat roughly 300 lines as a practical upper bound for a single Markdown file unless there is a strong reason not to split it.

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
- If `docs/plans/` grows, keep one plan per initiative or split into subfolders with a local `README.md`.
- If `python/tentgent-daemon/` grows, add a subtree `README.md` or `AGENTS.md` to route backend-specific reads.
- Keep the Python subproject in standard `pyproject.toml + src/` layout so IDE and packaging behavior remain predictable.
- If a `src/` subtree gains local rules, add a subtree `AGENTS.md` and link to it from this file.
