# Issue #132: Final Validation

Date: `2026-10-01`. Branch: `bug/132-server-runtime-option-noops`.
Scope and decisions: [implementation plan](./issue-132-server-runtime-option-contract-plan.md).
Implementation and Rust 1.99.0 revalidation passed the initial matrix. A
subsequent release-hardening audit found additional blockers; the current
branch is not yet approved for release. See the
[maintenance release checklist](./bugfix-maintenance-plan.md#release-reassessment-2026-10-03).

Current external gate: [RC3](https://github.com/HiroLiang/tentserv-agent/actions/runs/37050779331)
finished with all four native source, packaging and actual installed-artifact
checks passed. Both macOS architectures passed signature verification, but
Apple notarization returned HTTP 403 for a required missing/expired agreement
on both. The Account Holder must resolve
the agreement in the [Apple Developer account](https://developer.apple.com/account/)
before retrying notarization. Do not bypass this gate, merge the release PR,
promote stable, update Homebrew, or close #132 while acceptance is missing.
The release-creation job was skipped. No RC GitHub Release has been published;
latest stable remains `v1.1.1`. Workflow artifact installation is not a
substitute for the pending published-release MLX and Homebrew verification.

## Release Audit Follow-Up

On `2026-10-03`, public runtime ingress and transient Cluster reload recovery
were corrected in `deac594`. Exact route allowlisting preserves public DTO
validation; upstream redirects are rejected. A rollback waiting on old stream
leases retries without another definition edit, while unknown/terminal native
loads are not retried. The unknown-completion subprocess case proves the
documented stop, settle, reconcile, restart sequence actually clears claims.

Focused results: Local Rust 34 passed, Cluster Rust 42 passed/1 existing manual
ignored; Local subprocess 10 passed, Cluster reload subprocess 6 passed. Full
Rust workspace: 920 passed, the same 9 ignored entries. All-target check and
format passed without compile warnings. The initial sandbox-only run failed
to bind a loopback test listener; the permitted rerun executed successfully.

Packaging guards passed 8 offline tests plus the existing readiness suite.
Model asset safety and platform imports were corrected in `c7c56be`: the
supported Python 3.12 full-profile environment passed 178 tests + 11 subtests,
150-package dependency compatibility checks, and native backend imports.
The updated Rust dependency graph passed 922 workspace tests (9 ignored),
all-target checks without warnings, and an exact-version 313-package OSV audit.
See [dependency review](../development/dependency-security.md) for the two
remaining Python upstream advisories and tested application boundaries.

Real MLX lifecycle reran on Python 3.12.13 with the same pinned fixture: all
three tests passed in 48.2 seconds. Model counts dropped to zero, health polls
did not prevent process exit, restart changed the PID/token, and all isolated
processes were cleaned up. Native platform CI and installed-release smoke
remain pending; the older evidence below is retained as historical baseline.

The final local native-source gate also passed: warning-denying Rust debug
checks/tests (922 passed, 9 ignored), a fresh Python 3.12 base/dev environment
(177 passed, 1 optional test skipped, 11 subtests), all 27 lifecycle subprocess
cases, 6 artifact-smoke unit cases, and release-readiness fixtures. Optimized
Rust release tests separately passed the same 922/9 matrix with `-D warnings`.
Python 3.11 minimum-version base/dev checks passed the same 177/1/11 matrix.
The local RC archive passed the real isolated installer: SHA, version/layout,
managed Python 3.12.13, installed CLI bootstrap resolution, non-editable
site-packages imports and all 33 base/dev dependencies checked successfully.
The first Windows PR ownership run also passed before the final probe changes.

Final cross-platform review corrected the Windows server liveness fallback,
made `tasklist` PID parsing exact and fail-closed, and rejected Windows-rooted
model shard paths before joining. Existing identity and REST conflict tests
remain enabled; nine parser and four path-semantic regressions were added.
Python 3.12 full now passes 182 tests + 11 subtests; Python 3.11 base passes
181 tests + 11 subtests with one optional Transformers skip. Native Windows
live/exited-process tests and the final four-platform release gate remain
required. Windows termination/bootstrap parity is not added by this patch.
The final Rust debug and optimized release matrices both passed 931 tests
with the same 9 ignored entries; warning-denying all-target checks also passed.

## Native RC Feedback

[PR #139](https://github.com/HiroLiang/tentserv-agent/pull/139) is open; the
first candidate tag is `v1.1.2-rc.132.1` at `400f439`.
[RC1 workflow](https://github.com/HiroLiang/tentserv-agent/actions/runs/37044944968)
blocked publication before packaging. No RC Release or stable update was made.

- Linux: a startup test inferred the iteration from a request count after a
  25 ms sleep. Under load, the second request had not arrived yet, so the test
  mistook the previous valid proof for an early write. Request/completion
  handshakes now test the invariant deterministically; 100 repeated runs pass.
- Windows: both native live/exited server probes and process identity tests
  passed. REST/Cluster tests exposed unescaped Windows paths in hand-written
  TOML fixtures; fix the fixtures, not production metadata serialization or
  expected HTTP errors. The full native suite remains a required rerun.
- macOS ARM and Intel: all Rust/Python/lifecycle cases passed, then readiness fixtures
  failed because the runner did not include `rg`. Release jobs now install
  ripgrep explicitly. No readiness assertions were skipped.

The next candidate uses a new tag (`v1.1.2-rc.132.2`); never move the failed
candidate tag. Installed real-model smoke can explicitly select the packaged
Python project as well as the installed CLI, with Python path overrides cleared.
Its health-poll loop no longer issues a duplicate request that could race the
expected runtime exit; the rerun passed all three real MLX cases in 47.6 seconds.
Rust debug and optimized release regression after the fixture fixes each
passed 933 tests (9 ignored).
A fresh local Python gate exposed a cached non-editable wheel from before the
Windows path fix. Source validation must rebuild the runtime package and verify
installed source contents before accepting its test results; the failed run is
not counted as a passing Python matrix. Forced rebuilding now produces an
85-file exact source match; six source-guard regressions pass. The refreshed
base suite passes 181 tests, one optional Transformers skip and 11 subtests;
the full-profile source suite passes 182 tests and 11 subtests.
The final local native-source gate then passed end to end: the refreshed Python
matrix, all 27 lifecycle subprocess cases, both six-case artifact/source guard
suites and release readiness. Native RC2 and published-artifact checks remain
required before merging or promoting to stable.

RC2 (`11eb52d`, [workflow](https://github.com/HiroLiang/tentserv-agent/actions/runs/37048494235))
reached the Linux kernel suite and blocked publication on three stale-process
tests. Their invalid PID sentinel exposed a real Unix process-probe boundary,
not another timing assumption. The follow-up validates individual PID bounds
before probing or signaling and uses typed OS errors for existence checks.
The same boundary must cover daemon/server termination and training probes;
tests must never send real termination signals to invalid PIDs. The next
candidate will use a new tag after focused and full validation.
macOS ARM and Intel again passed source/lifecycle tests, then their optional PowerShell
dry-run exposed a POSIX-host fixture assumption (`LOCALAPPDATA`). The dry-run
now supplies an explicit isolated prefix/runtime home; installer defaults and
the actual Windows install path are unchanged.
Windows PR ownership checks passed. The RC2 daemon suite passed its previous
42 failures; one newly added fixture test compared two equivalent Windows path
spellings as different strings. Its expected value now uses the exact input
spelling to test lossless serialization. Kernel/Python/artifact checks on
Windows still require the next native run.

The PID follow-up passes 17 focused ownership tests and the complete 478-test
kernel library suite locally. Debug and optimized release workspace matrices
each pass 949 tests with the same 9 ignored entries; source-matched Python base
remains 181 passed, one optional skip and 11 subtests. The complete source gate,
all 27 lifecycle subprocess cases and release readiness passed. All three real
MLX cases passed again in 48.0 seconds, with zero remaining test processes.
Native Unix/Windows validation and RC3's installed-artifact checks remain
required; local PowerShell is unavailable, so the POSIX PowerShell dry-run
correction still needs its real runner check.

RC3 subsequently passed the complete native source gate on every platform,
including both real macOS PowerShell dry-runs. The separate repeated Windows
PR gate exposed a concurrency-test assumption: a bounded coordinator attempt
may return `Busy`. Test-only `8fb389a` now mirrors the existing supervisor's
20-second/150-ms retry policy, retains strict one-generation/first-policy
assertions, and adds forced-contention/deadline regressions. No production
locking or timeout changed. This file is `#[cfg(test)]`; it is the only source
difference from RC3. Local debug/release matrices each pass 951 tests (9 ignored);
the updated native Windows PR gate passes three ownership runs of 31 tests each.

## Compiler Baseline Follow-Up

Closeout audit on `2026-10-02` exposed a gap in the original matrix: it ran with
Rust 1.96.0, not the declared 1.81 minimum. An actual 1.81 workspace check failed
on `Option::is_none_or`; Clippy also identified its use in Cluster startup.
The user approved raising local, workspace, CI/release and minimum versions
together, selecting stable 1.99.0 instead of the initially considered 1.98.1.
Edition 2021, the dependency lockfile and runtime behavior remain unchanged.

Revalidation completed on `2026-10-03` using rustc 1.99.0
(`b940084d7`, LLVM 23.1.1) and Cargo 1.99.0. All four workspace packages inherit
the new minimum; the repository pin, Windows gate and both native release
build jobs select 1.99.0. The local default was verified outside the repository
as well. Existing toolchains are retained for rollback, not used by this branch.

| Check | Result |
| --- | --- |
| `cargo check --workspace --all-targets --locked` | Passed, no compile warnings; repeated after component installation completed. |
| `cargo fmt --all -- --check` | Passed, no source formatting changes. |
| `cargo clippy --workspace --all-targets --locked -- -A clippy::all -D clippy::incompatible_msrv` | Passed; this checks MSRV, not the full Clippy style backlog. |
| `cargo test --workspace --locked -q` | 917 passed, 9 existing ignored entries. |
| `cargo test --workspace --release --locked -q` | 917 passed, the same 9 ignored entries. |
| Debug CLI/daemon and `cargo build --workspace --release --locked` | Passed; release CLI version and daemon help also executed successfully. |
| `cargo check -p tentgent-platform-fs --all-targets --target x86_64-pc-windows-msvc --locked` | Passed; cross-compilation check only, not native Windows execution. |
| Python regression | 122 passed, 11 subtests passed. |
| Local startup / Cluster startup / Cluster reload scripts | 8 / 11 / 5 passed with rebuilt debug binaries. |
| Real MLX lifecycle script, same fixture below | 3 passed in 46.5 seconds; zero remaining test processes in every case. |
| Release readiness / workflow syntax / shell syntax | Passed; optional PowerShell dry-run still skipped. ShellCheck excludes only pre-existing intentional literal-string SC2016 notices. |

The live rerun again observed model count 0 after eager zero-retention loading
and real inference, successful restart with a new PID/token, lazy first use,
and shared 4-second model / 12-second runtime retention. Resource count fell
from 1 to 0 under health polling, then the runtime exited. Exact RSS is not an
acceptance threshold. No new model download was needed.

The toolchain consistency check also rejected floating-channel, MSRV-mismatch
and release-version-mismatch fixtures. Native Windows/Linux CI execution,
publication and the existing remaining boundaries below are still pending.

## Review Checkpoints

| Step | Commit | Result |
| --- | --- | --- |
| 1 | `8a28a68` | Input presence, target validation, Cloud compatibility, image lazy-only. |
| 2 | `bb283fa` | Managed preload tasks, scoped cleanup, internal lifecycle isolation. |
| 3 | `0316f41` | Local eager startup and truthful readiness/proof. |
| 4 | `f6b1780` | All-route Cluster eager startup and protected preload claims. |
| 5 | `1217166` | Committed/candidate snapshots, revision/disk comparison, ABA tests. |
| 6 | `3346c5e` | Asynchronous candidate preload, atomic admission/promotion, stream drain. |
| 7 | This evidence commit | Final regression/live matrix, help, docs and release notes. |

Each checkpoint passed its focused tests, compile/format checks and internal
diff review before the next step. The plan retains intermediate evidence and
test-fixture corrections; this file records the final state.

## Final Automated Matrix

Run from the repository root. Subprocess suites must run sequentially.

| Command | Final result |
| --- | --- |
| `cargo test --workspace -q` | 917 passed across workspace targets; 9 ignored entries listed below. |
| `cargo check --workspace --all-targets` | Passed, no compile warnings. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo build -p tentgent-cli -p tentgent-daemon --bins` | Passed. |
| `uv run --project python/tentgent-model-runtime pytest python/tentgent-model-runtime/tests -q` | 122 passed + 11 subtests. |
| `uv run --project python/tentgent-model-runtime python scripts/test-local-server-startup.py` | 8 passed. |
| `uv run --project python/tentgent-model-runtime python scripts/test-cluster-server-startup.py` | 11 passed, including the real 30-second drain budget. |
| `uv run --project python/tentgent-model-runtime python scripts/test-cluster-server-reload.py` | 5 passed. |
| Live command below | 3 passed; final rerun 52.1 seconds. |
| `bash scripts/test-release-readiness.sh` | Passed; optional PowerShell dry-run skipped (`pwsh` absent). |
| `git diff --check` | Passed. |

Ruff check/format passed for the new/changed smoke scripts and new image workflow
tests. The nine ignored Rust entries are one manual watcher benchmark, one
subprocess lock-holder helper (invoked by its parent test), and seven opt-in
macOS Keychain probes. Zero-test documentation targets are not test evidence.

## Decision And Acceptance Coverage

| Decisions | Evidence |
| --- | --- |
| D1, D6 | Local/Cluster startup unit and subprocess suites cover stored mode, eager/lazy admission, terminal failure, foreground/detached and both hidden hosts. Real MLX demonstrates load-then-release and lazy first use. |
| D2 | Five local Cluster routes, resolve-all-before-load, exact physical-key grouping, optional/provider limits and partial failure; no image route added. |
| D3 | Immutable committed reads, B/C supersession and ABA, failure fallback, staged claims surviving old traffic, paused snapshot selection/admission retry, old stream EOF, stop, and transient busy-release retry. |
| D4, D5 | Kernel/REST input-presence tests include explicit false/zero/null, inferred Cloud targets, canonical identity, legacy read/start/ref and inspect applicability. Local/Cluster aliases/null defaults remain covered. |
| D7 | Preload task activity, timeout/disconnected waits, failed-load retry and quarantine tests; live reused PID retains first-spawner idle policy and fake runtime records a second preload. |
| D8 | Existing issue/branch and #128/#130 dependency order retained; no proof-v2 or adapter-preload expansion. |
| D9 | Both backend formats reject new/stored eager inputs before launch. Python startup acquires no image workflow; eight dispatch cases select only the requested workflow, release its lease, and retain MLX Control's unsupported result. |

Readiness remains separate from process existence. Tests cover REST wait and
non-wait, observation expiry without premature Verified/Failed proof, terminal
worker-owned proof, stale/unsupported preload endpoints, and internal lifecycle
namespace rejection. Uncertain accepted work retains protection instead of
being cancelled or reported as freed.

## Real Model And Resource Evidence

User authorized one small download into the ignored `.tentgent-test` store.
Model: [mlx-community/Qwen2.5-0.5B-Instruct-4bit](https://huggingface.co/mlx-community/Qwen2.5-0.5B-Instruct-4bit),
revision `a5339a4131f135d0fdc6a5c8b5bbed2753bbe0f3`, 276.2 MiB imported.
Backend/environment: Apple Silicon macOS 27.0.1, Python 3.13, MLX 0.31.2,
mlx-lm 0.31.3, profile `local-chat-mlx-v1`. No fake backend is used in this suite.

With the dedicated store and project Python environment configured:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" \
TENTGENT_DATA_ROOT="$PWD/.tentgent-test" \
TENTGENT_PYTHON_DIR="$PWD/python/tentgent-model-runtime" \
TENTGENT_PYTHON_ENV_DIR="$PWD/.venv" \
  ./target/debug/tentgent model pull mlx-community/Qwen2.5-0.5B-Instruct-4bit \
  --revision a5339a4131f135d0fdc6a5c8b5bbed2753bbe0f3 --capability chat

uv run --project python/tentgent-model-runtime python scripts/test-server-lifecycle-live.py \
  --data-root "$PWD/.tentgent-test" --model-ref <full-ref-returned-by-pull>
```

The original pull resolved this exact revision before downloading. The pinned
command above makes the same snapshot reproducible. The test script creates
temporary control homes and uses the supplied model store, never production
server/Cluster state. The environment must already contain the MLX backend.

| Observation | Resource count | Final rerun RSS (KiB) |
| --- | --- | --- |
| Eager ready, model idle 0 | 0 | 446,752 |
| Real chat completed, model idle 0 | 0 | 560,416 |
| Same proxy after runtime idle restart | 0, new PID/token | 421,872 |
| Lazy first real chat completed | 0; no Python generation before request | 526,304 |
| Positive model idle 4, after preload | 1 loaded, 0 active leases | 759,616 |
| Second eager proxy reuses generation | 1; original 12/4-second policy retained | 759,632 |
| Positive retention expired under health polling | 0 | 425,328 |

Real replies were non-empty ("Hello! How may I assist you today?"). Continuous
proxy/Python health polling did not postpone 12-second runtime idleness; the
runtime port closed and PID disappeared after graceful shutdown. A later chat
started a new PID/token and succeeded. Every case's cleanup asserted zero
remaining test processes. Temporary homes were removed; the authorized model
and proofs remain in the ignored test store for reuse.

Resource release is not an exact RSS threshold: framework/allocator memory can
remain after model count reaches zero. This test confirms logical resource
release, real reload/reuse, and OS process exit, not per-allocation GPU telemetry.

## Remaining Boundaries

- Native Windows/Linux execution and their CI gates were not run on this Mac.
- Image dispatch uses fake backends/pixel decoding; no Diffusers/MFLUX weights
  or paid Cloud inference were downloaded/executed.
- Confirmed preload failure releases only candidate resources. Unknown
  completion intentionally retains claims; stop its owning server, wait for
  accepted work, reconcile, then restart.
- No architecture cleanup, proof-v2 migration or ownership redesign is included.
- Version preparation targets `v1.1.2`; PR/merge, native release verification,
  Homebrew update, and publication are separate pending gates.
- The GitHub issue remains open for review/merge; this local record supersedes
  its earlier "implementation has not started" progress prose.

## Draft PR

Title: `fix: honor server load modes and safely stage Cluster reloads (#132)`

Body:

- Honor Local/Cluster eager startup with truthful readiness and worker-owned proof.
- Preload Cluster candidates before guarded promotion; preserve old traffic and
  streaming ownership on failure or supersession.
- Reject unsupported new Cloud lifecycle inputs while retaining old specs/refs;
  enforce lazy-only Diffusers and MLX/MFLUX image servers.
- Preserve #131 idle defaults, shared-generation ownership and scoped cleanup.
- Include full Rust/Python regression, 27 subprocess and 3 real MLX smoke cases,
  aligned contracts/help and version notes.
- Align the local compiler, workspace minimum and CI/release builds on Rust
  1.99.0 and managed Python on 3.12; patch known dependency advisories, enforce
  model asset safety, and validate native packages before publication.

Validation and limits: link this record in the PR using `Refs #132`. Close the
issue after installed stable release and Homebrew verification, not on merge.
