# Issue 131 Model Idle Release And Runtime Keep-Alive Plan

Status: implemented and validated on `bug/131-model-idle-release`, merged into
`main`, and awaiting `v1.1.1` release-candidate verification for
[#131](https://github.com/HiroLiang/tentserv-agent/issues/131).

Parent plans:

- [Bugfix And Maintenance Plan](./bugfix-maintenance-plan.md)
- [v1.2.0 Local Compatibility State Plan](./v1.2.0-local-compatibility-state-plan.md)

Branch: `bug/131-model-idle-release`

Priority: complete this maintenance prerequisite before resuming `#127` feature
implementation. The `#127` branch and its issue-level plan remain independent.

## Outcome

Restore bounded, documented local model-memory behavior without weakening
shared-runtime ownership or request safety.

When this issue is complete:

- a finished request owns no model lease;
- an unused model is released according to an explicit model-idle policy;
- runtime process shutdown follows a separate process-idle policy;
- health and ownership probes observe lifecycle state without refreshing user
  or model activity;
- local and Cluster proxies can reuse or restart the physical runtime safely;
- MLX release drops model references and clears MLX caches;
- CLI, REST, persisted server specs, inspect output, contracts, and tests agree
  on the meaning of each idle setting.
- no supported entry point can select an infinite model or runtime timeout.

## Observed Incident

The issue was reproduced with one lazy-loaded local MLX chat server configured
with `--idle-seconds 30`.

| Observation | Evidence | Meaning |
| --- | --- | --- |
| Request lease | `active_leases = 0` | Request cleanup completed; this is not an active-lease leak. |
| Runtime tasks | `task_count = 0` | No inference task remained active. |
| Model resource | `loaded = true` after more than 356 idle seconds | The loaded model was retained beyond the requested interval. |
| Effective model policy | `model_idle_timeout_seconds = -1` | Automatic model release was disabled. |
| Python process footprint | about 3.26 GiB | The physical runtime retained the model allocation. |
| MLX/IOAccelerator footprint | about 2.85 GiB | Most retained memory was Apple unified GPU memory, not the Rust proxy. |
| Rust proxy RSS | about 11 MiB | The proxy is not the material memory owner. |

The incident proves that model release was not reached. It does not prove that
MLX leaks memory after `release()` runs, because that method was never invoked
for the retained resource.

## Regression History

The original direct Python runtime used
`model_idle_timeout_seconds = 0.0`. Its health route returned a snapshot without
changing activity. A model lease ending at timeout zero therefore called
`release_idle()` immediately.

Commit `a30ba32` (`Route local model runtimes through HTTP`) introduced the
Rust-managed shared Python runtime and made two intentional warm-runtime
changes:

- Rust launched the model runtime with `--model-idle-timeout-seconds -1`;
- `GET /healthz` began calling `task_manager.touch_activity()` so supervisor
  polling could keep the runtime process alive.

Commit `a466487` (`Use Rust proxy for local model servers`) then routed the
stored `idle_seconds` setting to `idle_keep_alive_seconds` only. Commit
`da4bf8e` (`#118`) preserved both values as first-spawner runtime ownership
policy; it did not originate the fixed `-1` behavior.

The regression is therefore a policy and contract mismatch introduced by the
shared-runtime transition, not a request-finally or MLX-release implementation
failure.

## Current Failure Path

1. CLI short help says `--idle-seconds` releases a loaded model.
2. CLI long help, user docs, and the daemon contract describe it as Python
   runtime idle shutdown.
3. `LocalServerRuntimeConfig.idle_seconds` is mapped through
   `ModelRuntimeDaemonLaunchPolicy::with_idle_keep_alive_seconds`.
4. That constructor changes only process keep-alive and always copies the
   `-1` model timeout constant.
5. Rust passes both values to the Python daemon.
6. `ResourceManager.release_idle()` skips every resource whose timeout is
   negative.
7. The Rust supervisor polls `/healthz` every 30 seconds, and the health route
   refreshes task-manager activity, so the process timeout is also postponed.
8. The request lease reaches zero, but neither the model release nor the
   runtime shutdown path reclaims the MLX allocation.

## Required Invariants

- Request success, error, cancellation, stream drop, and task failure release
  the request lease exactly once.
- A model is never released while `active_leases > 0` or while its resource
  lock is held by execution.
- Model idle age begins after the final lease finishes, not when a request is
  submitted.
- Process idle age is based on accepted workload, not health, inspect, or
  ownership probes.
- Model release and process shutdown remain distinct transitions.
- Process shutdown calls `release_all()` even when model-idle release has not
  happened yet.
- A route claim or listening Rust proxy does not by itself require the model
  to remain loaded.
- A later request may reload a released model or restart an exited physical
  runtime without changing the logical server spec.
- The first spawner's complete launch policy remains durable and mismatch
  diagnostics compare both idle values.
- Cloud server idle behavior does not change accidentally while fixing local
  and Cluster model runtimes.

## Scope Lock

### Included

- Public and stored semantics for process idle and model idle.
- Canonical runtime-idle and model-idle names plus the legacy runtime alias.
- Backward-compatible server-spec and daemon REST handling.
- Local model-bound and Cluster runtime launch-policy mapping.
- Pure health/liveness observation.
- Python resource-manager and lifecycle regression tests.
- Rust policy, ownership, server identity, CLI, daemon, and Cluster tests.
- MLX chat release and reload proof, plus shared resource-manager coverage for
  the other model capabilities.
- Inspect and diagnostic visibility for both effective idle policies.
- Contract, command, runtime, and API documentation updates.
- One repeatable local live-smoke runbook.

### Explicitly Excluded

- Compatibility tuple and proof v2 work from `#127`.
- GPU, CPU, or memory scheduling.
- Cross-model eviction, LRU policy, or memory-pressure callbacks.
- Changing runtime ownership identity or route-claim architecture.
- New backend families or model conversion.
- Exact Activity Monitor byte guarantees after cache release.
- Provider or cloud process-lifecycle redesign.
- Correcting ignored `lazy_load` and Cloud lifecycle fields; that follow-up is
  tracked by [#132](https://github.com/HiroLiang/tentserv-agent/issues/132).

## Proposed Lifecycle Contract

| State or clock | Reset by | Not reset by | Expiry effect |
| --- | --- | --- | --- |
| Request lease | Reserve a model for one execution | Health, inspect, ownership polling | Finalization decrements `active_leases`. |
| Model idle | Final model lease completion | Health and unrelated runtime tasks | Remove the resource and call backend `release()`. |
| Runtime process idle | Accepted runtime workload | Health, inspect, ownership polling | Begin graceful process shutdown, then `release_all()`. |
| Proxy lifecycle | Server start/stop | Python model release | Keep or close the public server port independently. |
| Ownership generation | Physical runtime start/reuse/exit | Model unload/reload inside the same process | Record and reconcile the physical process only. |

## Accepted Decisions

These choices were accepted on `2026-08-07` and are implementation inputs, not
open alternatives.

### D131-01: Keep Two Independent Clocks

Do not use one task-manager timestamp for both model release and runtime
shutdown. The resource manager already owns model `last_used_at`; the task
manager owns process workload activity.

### D131-02: Use Canonical Names And One Legacy Alias

The canonical public controls are:

- `--runtime-idle-seconds` / `runtime_idle_seconds`;
- `--model-idle-seconds` / `model_idle_seconds`.

The existing `--idle-seconds` / `idle_seconds` input remains a deprecated alias
for runtime idle only. Supplying legacy and canonical runtime values together
is valid only when the values match; conflicting values are a validation
error. The direct Python launch protocol likewise keeps
`--idle-keep-alive-seconds` and `--model-idle-timeout-seconds` as deprecated
aliases while new Rust launches use the canonical names.

### D131-03: Require Finite Validated Timeouts

- Runtime idle defaults to `300` seconds.
- Model idle defaults to `0` seconds, meaning release immediately after the
  final model lease completes.
- Both values must be finite and non-negative.
- The complete pair must satisfy
  `0 <= model_idle_seconds <= runtime_idle_seconds`.
- Negative values, including the former `-1` retain-forever sentinel, are
  rejected at Rust and direct Python boundaries.

An omitted field selects its default. Explicit zero is supported. A runtime
value of zero begins shutdown as soon as the final active task completes; its
model value must therefore also be zero.

### D131-04: Make Health A Pure Probe

Remove `touch_activity()` from `/healthz`. Startup probes, the 30-second Rust
poller, CLI inspection, and ownership reconciliation must be observational.
Accepted inference or training work remains the source of runtime activity.

### D131-05: Anchor Idle Clocks At Work Completion

Model idle begins when the final resource lease completes. Runtime idle starts
at readiness when no task has run, then re-anchors when the final accepted task
completes and no task is active. Submission, execution, and completion count as
workload; retained task-result metadata does not postpone shutdown.

### D131-06: Preserve Existing Specs Without Changing Their Meaning

Existing `idle_seconds` data continues to mean runtime idle. A stored spec with
no model-idle field receives the new effective default of zero. Explicit
default values are normalized so omission and explicit defaults do not produce
different server identities. Readers accept the legacy field and new stored
writes use canonical field names.

Because `/v1/servers` is a stable API surface, REST responses keep deprecated
`idle_seconds` as a mirror of the requested runtime override while also
returning canonical optional fields. Server inspection reports separate
concrete effective values so an omitted setting is visibly `300` / `0` without
changing the stored requested-value shape.

### D131-07: Preserve Resource-Manager Safety

Keep the current non-blocking idle-release selection, active-lease guard, and
per-resource lock. Add tests before changing policy wiring; do not rewrite the
manager unless a failing test exposes a separate race.

### D131-08: Fix The Shared Boundary, Then Verify Backends

The timeout bug affects every model capability using `ResourceManager`, even
though MLX chat made it visible. Fix policy wiring once. Verify that each
concrete loaded backend clears owned references; require MLX implementations
that allocate MLX resources to call `clear_mlx_cache()`.

### D131-09: Test Observable Lifecycle State, Not Exact OS Bytes

Automated tests assert resource count, loaded state, release calls, process
state, generation replacement, and successful reload. A macOS smoke test may
record `footprint` as evidence, but exact byte reclamation is not a stable CI
assertion.

### D131-10: Preserve Complete First-Spawner Policy

Runtime ownership continues to persist and compare process-idle plus
model-idle policy. Reuse never silently overwrites a live generation's policy;
inspect reports any mismatch.

### D131-11: Keep Target Option No-Ops In A Follow-Up Bug

Issue #131 covers Local, Cluster, and one-shot managed Python runtime timeout
behavior. The already-observed ignored `lazy_load` propagation and unsupported
Cloud lifecycle fields are tracked in
[#132](https://github.com/HiroLiang/tentserv-agent/issues/132). They do not
expand #131 unless a focused regression test proves they block the accepted
release and restart contract.

## Decision Completeness Check

No user product decision remains open for #131. Implementation should stop and
reopen this register only if evidence contradicts one of these locked rules.
The following implementation details are intentionally delegated to the
smallest compatible code shape:

- use integer seconds on public Rust CLI, REST, and stored-spec surfaces while
  allowing shorter internal durations in tests;
- accept matching legacy and canonical runtime values, reject conflicts, and
  never select one silently;
- keep direct one-shot commands such as chat, embedding, rerank, audio, vision,
  and image on the `300` / `0` defaults rather than adding duplicate flags;
- normalize default-valued policy input before server identity generation;
- keep the Rust public proxy listening until explicit server stop even if its
  subordinate Python runtime unloads a model or exits and later restarts.

Questions about eager loading or Cloud target options belong to #132, not this
implementation.

## Accepted Entry-Point Mapping

| Entry point | Runtime policy source | Model policy source | Lifecycle boundary |
| --- | --- | --- | --- |
| `tentgent daemon` | None | None | The Rust management daemon loads no model and has no model-runtime idle flags. |
| `tentgent server run` for Local | Canonical option, legacy runtime alias, or `300` | Canonical option or `0` | The public Rust proxy remains; its managed Python runtime may exit and restart. |
| `tentgent cluster run` | Same policy applied to each Local target runtime | Same policy applied to each Local target runtime | Cluster routing remains available while subordinate runtimes release or restart. |
| Daemon `POST /v1/servers` | Canonical field, legacy runtime alias, or `300` | Canonical field or `0` | Validate the effective pair before saving or starting. |
| One-shot chat, embedding, rerank, audio, vision, video, image, and support-verification work | `300` | `0` | No duplicate public flags; an adopted shared runtime still obeys first-spawner policy. |
| LoRA tuning through a Local runtime | Server runtime policy | Resource-specific `0` | The active tuning task postpones runtime idle; its model lease releases on completion. |
| Direct Python runtime CLI | Canonical flag or deprecated launch alias | Canonical flag or deprecated launch alias | Reject negative and invalid pairs before serving. |
| Cloud server target | Unchanged by #131 | Not applicable | Ignored target options and validation are owned by #132. |

## Implementation Plan

### Phase 1: Lock Contract And Regression Tests

- [x] Resolve public names, defaults, validation, compatibility, and scope.
- [x] Update the issue acceptance criteria to the selected public semantics.
- [x] Add focused resource-manager tests for zero and positive model timeouts,
  rejected negative values, active leases, release, and reload.
- [x] Add a health-route test proving repeated probes do not touch activity.
- [x] Add lifecycle tests proving workload postpones process shutdown while
  observation does not.
- [x] Change no production behavior until the regression tests fail for the
  expected reason.

### Phase 2: Add Explicit Server Policy Data

- [x] Add canonical runtime-idle and model-idle fields to CLI, kernel request,
  `ServerSpec`, daemon DTO, REST request/response, and Local/Cluster runtime
  configuration.
- [x] Preserve `idle_seconds` as a deprecated runtime input alias and preserve
  deserialization of existing specs.
- [x] Keep the stable REST response mirror and expose concrete effective values
  in detailed inspection.
- [x] Validate the complete pair at CLI, REST, stored-spec, kernel, and Python
  launch boundaries.
- [x] Normalize omitted and explicit defaults before deterministic server
  identity generation.
- [x] Avoid changing Cloud behavior; its ignored options are owned by #132.
- [x] Render both values in detailed server inspection without widening compact
  list output unnecessarily.

### Phase 3: Refactor Runtime Launch Policy

- [x] Replace `with_idle_keep_alive_seconds` with constructors or a typed input
  that sets process idle and model idle deliberately.
- [x] Remove the model-timeout `-1` default and all supported negative-timeout
  paths.
- [x] Pass both selected values to the Python daemon.
- [x] Preserve first-spawner storage, reuse, mismatch diagnostics, and
  ownership tests for the complete pair.
- [x] Apply the same local model policy to native local and Cluster routes.

### Phase 4: Separate Health From Workload Activity

- [x] Remove activity mutation from `/healthz`.
- [x] Confirm every accepted runtime task still updates process activity.
- [x] Anchor initial runtime idle at readiness and later idle at final task
  completion; confirm retained task metadata cannot postpone it.
- [x] Preserve `closing` visibility long enough for the Rust supervisor to mark
  ownership state before exit.
- [x] Verify an exited idle runtime is removed or reconciled and can be started
  again by the proxy.

### Phase 5: Verify Release And Reload

- [x] Verify a positive model timeout leaves the resource loaded before expiry.
- [x] Verify expiry removes the resource only after `active_leases == 0`.
- [x] Verify MLX chat clears model, tokenizer, adapter state, and MLX cache.
- [x] Audit loaded audio, vision, video, image, Transformers, llama.cpp, and
  other concrete backends for owned-reference cleanup.
- [x] Verify a request after model release reloads and succeeds in the same
  runtime process.
- [x] Verify a request after process shutdown starts a new ownership generation
  and succeeds.

### Phase 6: Align Contracts And Operator Surfaces

- [x] Update `docs/contracts/model-runtime-server.md` with the two-clock
  contract and pure health semantics.
- [x] Update `docs/contracts/http-daemon.md` and `docs/user/api.md` for the
  canonical fields and deprecated runtime alias.
- [x] Update `docs/user/commands.md` and CLI help with copyable examples.
- [x] Update runtime ownership documentation only where first-spawner policy or
  probe semantics need clarification.
- [x] Update version notes if the behavior is user-visible in the next release.

### Phase 7: Validate And Record Evidence

- [x] Run focused Python resource and lifecycle tests.
- [x] Run focused kernel runtime-policy, ownership, server-spec, and identity
  tests.
- [x] Run focused CLI parsing/rendering and daemon REST tests.
- [x] Run local and Cluster server lifecycle tests.
- [x] Run workspace and Python suites.
- [x] Execute the live MLX smoke runbook below.
- [x] Record the final effective policies and before/after lifecycle snapshots
  below without storing prompts or generated content.

## Implementation Evidence

Validation completed on 2026-08-08 with effective policy defaults of `300`
runtime-idle seconds and `0` model-idle seconds.

The release gate was repeated on 2026-08-25. The full Rust and Python suites
passed again, the live MLX release/reload/restart sequence completed without a
zombie, and the strengthened Cluster smoke ended with zero claims, generations,
operations, stale records, and malformed records.

- Python: 42 tests passed, including seven subtests for policy parsing,
  pure-health observation, lease safety, release, reload, and shutdown.
- Rust: the complete workspace suite passed with 110 CLI, 307 daemon, and 443
  kernel tests; one daemon and one kernel test remain intentionally ignored.
- Live MLX chat: the loaded resource reached zero leases and zero tasks, then
  released in-process. RSS fell from about 3,043,776 KiB to 728,560 KiB before
  runtime shutdown; a later request reloaded successfully.
- Live process lifecycle: repeated health probes did not postpone shutdown; the
  Python runtime exited, was reaped without a zombie, and restarted on demand
  while the Rust proxy remained available.
- Cluster: chat, embedding, rerank, audio transcription, and vision chat passed
  through one detached proxy. Stop plus reconcile ended healthy with no stale
  ownership records.
- Compatibility: an exact legacy persisted model timeout of `-1` is retired and
  replaced safely. New canonical negative values and conflicting aliases are
  rejected.

## Expected Code Touchpoints

- `src/tentgent-cli/src/cli/commands/server.rs`
- `src/tentgent-cli/src/cli/server/`
- `src/tentgent-kernel/src/features/server/`
- `src/tentgent-kernel/src/features/runtime/infra/model_daemon/`
- `src/tentgent-kernel/src/features/runtime_ownership/`
- `src/tentgent-daemon/src/server/local/`
- `src/tentgent-daemon/src/server/cluster/`
- `src/tentgent-daemon/src/handlers/rest/server/`
- `python/tentgent-model-runtime/src/tentgent/runtime/server/`
- `python/tentgent-model-runtime/src/tentgent/runtime/backends/`
- focused Rust and Python tests adjacent to those boundaries
- the contract and user documents named in Phase 6

The exact file split should follow the nearest package structure. Keep
`mod.rs` and `lib.rs` as composition files.

## Live MLX Smoke Runbook

The accepted implementation must support this sequence:

1. Build the branch and bootstrap the current local-model runtime profile.
2. Start a lazy local MLX chat server with a short model-idle interval and a
   longer process-idle interval.
3. Send one OpenAI-compatible chat request.
4. Observe `active_leases = 0` and `task_count = 0` after completion.
5. Poll health through more than one Rust supervisor interval.
6. Observe the model resource disappear after the model-idle interval even
   though health probes continue.
7. Send a second request and verify successful in-process model reload.
8. Wait through the process-idle interval and verify graceful runtime exit.
10. Stop the server and verify no physical runtime or ownership generation is
    left stale.
11. On macOS, record `footprint` before release, after model release, and after
    process exit as diagnostic evidence only.

## Validation Commands

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
uv run --project python/tentgent-model-runtime pytest
bash scripts/test-cluster-runtime-ownership-smoke.sh
git diff --check
```

Use narrower tests during implementation, then run the complete gates before
closeout. The ownership smoke script may need a focused update if its current
one-second process-idle assumption changes under the accepted two-clock
contract.

## Risks And Guardrails

| Risk | Guardrail |
| --- | --- |
| A health probe still refreshes activity indirectly. | Test the task-manager timestamp across repeated health requests. |
| A model releases during execution. | Preserve active-lease and resource-lock guards; add concurrent expiry tests. |
| Existing specs silently change meaning. | Prefer an additive optional field and explicit target-specific mapping. |
| Cloud idle behavior changes with local model policy. | Keep Cloud mapping and regression tests separate. |
| A shared runtime reuses the wrong policy. | Persist and compare the complete first-spawner policy pair. |
| Runtime exits before closing is observed. | Preserve closing grace and supervisor ownership transition tests. |
| MLX cache evidence is mistaken for a deterministic byte guarantee. | Gate on resource/process state; record OS memory only as smoke evidence. |
| The fix grows into a scheduler. | Exclude eviction, pressure callbacks, and cross-model policy from `#131`. |

## Completion And Return To Issue 127

Close `#131` only when the selected contract, implementation, automated tests,
live smoke evidence, CLI/REST behavior, and documentation agree.

After the fix reaches `main`:

1. update the `#127` feature branch from the accepted `main` revision;
2. confirm the `#127` sub-plan still has no runtime-lifecycle scope;
3. resume proof v2 work without carrying `#131` implementation decisions into
   compatibility tuple identity unless a concrete execution fact requires it.

Move this document to `docs/plans/archive/` only after `#131` is complete and
the active maintenance plan no longer needs it as a handoff reference.
