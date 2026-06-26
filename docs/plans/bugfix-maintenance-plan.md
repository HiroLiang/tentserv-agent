# Bugfix And Maintenance Plan

Status: active post-`v1.0.0` maintenance and patch planning record.

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
| [#104](https://github.com/HiroLiang/tentserv-agent/issues/104) | Open | `v1.0.1 Patch` | Clean up stale post-1.0 roadmap wording and converge active plan routing. |
| [#105](https://github.com/HiroLiang/tentserv-agent/issues/105) | Open | `v1.0.1 Patch` | Investigate signed Homebrew macOS Keychain password prompt behavior and decide whether docs or credential reset/migration guidance is needed. |
| [#106](https://github.com/HiroLiang/tentserv-agent/issues/106) | Open | `v1.0.1 Patch` | Improve user-facing diagnostics when local model execution is blocked by missing runtime-required model files. |
| [#107](https://github.com/HiroLiang/tentserv-agent/issues/107) | Open | `v1.0.2 Patch` | Retain failed local model execution outcomes as inspectable support evidence through the existing file-backed proof store. Move to the `v1.x` roadmap only if later work requires a new durable proof store. |

## Candidate Maintenance Issues

These candidates are suitable for a patch milestone when their implementation
stays small and does not introduce a new product surface. Create GitHub issues
before implementation when a candidate is selected.

- No untracked candidates are currently listed. Add new maintenance candidates
  here only when they have not yet been opened as GitHub issues.

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
- serving target or cluster configuration
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
