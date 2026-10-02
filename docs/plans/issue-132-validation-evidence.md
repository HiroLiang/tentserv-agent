# Issue #132: Final Validation

Date: `2026-10-01`. Branch: `bug/132-server-runtime-option-noops`.
Scope and decisions: [implementation plan](./issue-132-server-runtime-option-contract-plan.md).
Implementation and Rust 1.99.0 revalidation are ready for human review, not
merged or released.

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
  completion intentionally retains claims; inspect/reconcile after work ends.
- No architecture cleanup, proof-v2 migration, ownership redesign, PR creation,
  push, merge, version bump, Homebrew update, or release publication occurred.
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
- Include full Rust/Python regression, 24 subprocess and 3 real MLX smoke cases,
  aligned contracts/help and Unreleased notes.
- Align the local compiler, workspace minimum and CI/release builds on Rust
  1.99.0; validate both debug and optimized release tests without changing
  edition or dependencies.

Validation and limits: link this record in the PR. `Fixes #132` after merge.
