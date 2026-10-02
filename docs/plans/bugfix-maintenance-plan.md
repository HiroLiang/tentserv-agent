# Bugfix And Maintenance Plan

Status: active post-`v1.0.0` maintenance and patch planning record. Issues
`#103`-`#107`, `#131`, and `#136` are complete. `#132` tracks the remaining
server-option no-op follow-up.

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
| [#131](https://github.com/HiroLiang/tentserv-agent/issues/131) | Completed; released in `v1.1.1` | `v1.2.0` | Restore explicit model-idle release and runtime process keep-alive semantics; prevent health polling from retaining an idle MLX model/runtime indefinitely. |
| [#132](https://github.com/HiroLiang/tentserv-agent/issues/132) | Steps 1-7 checked; release-hardening fixes in progress | `v1.2.0` (patch selection pending) | Local/Cluster startup, staged eager reload, Cloud validation and lazy-only images are implemented. See the [issue plan](./issue-132-server-runtime-option-contract-plan.md) and [validation](./issue-132-validation-evidence.md). |
| [#136](https://github.com/HiroLiang/tentserv-agent/issues/136) | Completed | `v1.2.0` | Make README task navigation lead directly to feature examples, parameters, and HTTP formats; preserve existing documentation links. |

## Current Handoff State

As of `2026-09-28`, `#131` is merged and released in `v1.1.1`; its evidence and
smoke procedure remain in
[issue-131-model-idle-release-plan.md](./issue-131-model-idle-release-plan.md).
Issue `#132` has a [decision register and review checkpoints](./issue-132-server-runtime-option-contract-plan.md).
Its decisions include lazy-only Diffusers and MLX/MFLUX image generation, with
implementation split into seven reviewable steps. As of `2026-10-01`, Steps 1-7
are implemented and checked, including integration, documentation and real MLX
load/release/restart evidence. Independent commits await human review; no push,
PR creation, merge or release was performed. GitHub issue status remains open.
It is independent of `#127` and precedes `#128` and `#130` implementation.

### Release Reassessment (`2026-10-03`)

The user authorized fixing release blockers, publishing when verified, and
updating Homebrew, including related maintenance issues. Target train:
`v1.1.2-rc.132.N` then `v1.1.2` from the same verified source. Feature issues
`#126`-`#130` remain in `v1.2.0`; they are not prerequisites for this patch.
The earlier matrix is baseline evidence, not approval of the new changes.
RC1 at `400f439` was blocked by test portability/environment findings. RC2 at
`11eb52d` reached Linux kernel tests and exposed an invalid-PID probe boundary;
fix and validate it before a new candidate. Neither candidate was promoted. See the
[native feedback](./issue-132-validation-evidence.md#native-rc-feedback).

RC3 at `ccc8351` passed all native source gates and reached notarization. Apple
returned HTTP 403 because a required agreement is missing or expired. This is
an Account Holder action, not a code/signing workaround. Keep PR #139 open and
stable/Homebrew at `v1.1.1` until notarization and remaining artifact checks pass.
Test-only `8fb389a` fixes bounded-Busy retry coverage; its Windows PR gate passes
and it does not change RC3 product sources.

Before release:

- Close public-to-internal runtime route bypasses with an exact public route
  allowlist and no upstream redirects. Preserve managed DTO validation.
- Retry only known transient Cluster ownership contention; keep terminal
  load failures memoized and retain unknown-work claims. Document the actual
  stop, settle, reconcile, restart recovery sequence.
- Remove implicit model-repository code execution, audit locked dependencies,
  and verify mitigations for advisories without upstream fixes. Document the
  trusted-host deployment boundary; no Internet-service security claim.
- Require explicit accepted Apple notarization, correct Windows runtime
  refresh, locked/version-consistent packages, and stable release metadata.
- Run affected/full regressions, native platform CI, artifact installation and
  real-model idle/restart tests. No skipped test counts as native validation.
- Publish RC, validate installed artifacts, then stable and the Homebrew tap.
  Close issues only after release and installed-version verification.

New milestone/issue creation awaits explicit approval after the permission
reviewer rejected that metadata write. No general PR CI implementation or
public vulnerability details are published while this approval is pending.

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
