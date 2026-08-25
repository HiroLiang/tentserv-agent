# Bugfix And Maintenance Plan

Status: active post-`v1.0.0` maintenance and patch planning record. Issues
`#103`-`#107` are completed; `#131` is implemented and validated pending
review and merge, and `#132` tracks a separate server-option no-op follow-up.

This plan tracks released-product cleanup: bugs, diagnostics gaps, stale
documentation, release follow-up, repository hygiene, and small hardening work.
New features and larger architecture work belong in
[v1.x-roadmap.md](./v1.x-roadmap.md).

## Purpose

Use this document to keep maintenance work visible without turning the
long-term roadmap into a bug queue. A maintenance item belongs here when it
improves an existing documented workflow, removes confusing stale wording, or
keeps release operations reliable.

## Triage Rules

- Use this plan for fixes to already documented behavior.
- Use this plan for diagnostics, `doctor`, install, upgrade, Homebrew, release,
  and documentation cleanup.
- Move the work to [v1.x-roadmap.md](./v1.x-roadmap.md) when the fix requires a
  new public API, new command family, new schema, or version-sized feature
  design.
- Keep issue details in GitHub. This file should summarize the maintenance
  queue and release boundary, not duplicate every issue body.

## Tracked Issues

Keep issue details in GitHub. This section only records the maintenance issue
queue that should stay visible from the active plan.

| Issue | Status | Milestone | Summary |
| --- | --- | --- | --- |
| [#103](https://github.com/HiroLiang/tentserv-agent/issues/103) | Completed | `v1.0.1 Patch` | Clean up local `.DS_Store` repository noise; resolved without tracked repository changes. |
| [#104](https://github.com/HiroLiang/tentserv-agent/issues/104) | Completed | `v1.0.1 Patch` | Clean up stale post-1.0 roadmap wording and converge active plan routing into this maintenance plan plus the active `v1.x` roadmap. |
| [#105](https://github.com/HiroLiang/tentserv-agent/issues/105) | Completed | `v1.0.1 Patch` | Fix signed Homebrew macOS Keychain prompt behavior and keep the release path aligned with the existing signing setup. |
| [#106](https://github.com/HiroLiang/tentserv-agent/issues/106) | Completed | `v1.0.2 Patch` | Improve user-facing diagnostics when local model execution is blocked by missing runtime-required model files. |
| [#107](https://github.com/HiroLiang/tentserv-agent/issues/107) | Completed | `v1.0.2 Patch` | Retain local model execution outcomes as inspectable `runtime-execution` support evidence through the existing file-backed proof store. |
| [#131](https://github.com/HiroLiang/tentserv-agent/issues/131) | Implemented; pending review and merge | `v1.2.0` | Restore explicit model-idle release and runtime process keep-alive semantics; prevent health polling from retaining an idle MLX model/runtime indefinitely. |
| [#132](https://github.com/HiroLiang/tentserv-agent/issues/132) | Planning | `v1.2.0` | Honor Local/Cluster lazy-load configuration and stop Cloud targets from silently accepting unsupported local-runtime lifecycle options. |

## Current Handoff State

As of `2026-08-08`, `#131` is implemented and validated on its bug branch. Its
detailed diagnosis, decisions, implementation evidence, and smoke procedure are in
[issue-131-model-idle-release-plan.md](./issue-131-model-idle-release-plan.md).
Review and merge that fix before resuming `#127` implementation. Issue `#132`
remains a separate follow-up and should receive its own issue-level planning
before its branch is implemented.

The completed `v1.1.0` Cluster issue flow is archived under
[archive/cluster-roadmap.md](./archive/cluster-roadmap.md). Future feature work
is routed through [v1.x-roadmap.md](./v1.x-roadmap.md).

## Candidate Maintenance Issues

These candidates are suitable for a patch milestone when their implementation
stays small and does not introduce a new product surface. Create GitHub issues
before implementation when a candidate is selected.

- Add a general pull-request CI gate that runs the Rust workspace, Python
  runtime tests, formatting, Clippy, and documentation/release-readiness
  checks. `#113` runs this matrix locally, but the repository currently has
  only the focused Windows ownership pull-request workflow. Open a maintenance
  issue before implementing this repository-wide CI change.

## Patch Boundary

A maintenance issue can stay in a patch milestone when it:

- fixes misleading docs or diagnostics
- improves a current error path without changing the public request shape
- improves release, install, or Homebrew repeatability
- adds narrow regression coverage for already promised behavior
- does not require a new public command, endpoint, model schema, or storage
  contract

Move it to the `v1.x` roadmap when it needs:

- a new compatibility proof store or durable schema
- cluster configuration
- cross-model scheduling or resource management
- provider tool orchestration
- cloud rerank provider adoption
- automatic multimodal context assembly
- conversion automation or generated model metadata

## Validation

Maintenance documentation changes should usually run:

```bash
rg "v1.x-roadmap|bugfix-maintenance-plan" README.md AGENTS.md docs
git diff --check
```

Runtime, CLI, or REST tests are required only when a maintenance issue changes
product behavior.
