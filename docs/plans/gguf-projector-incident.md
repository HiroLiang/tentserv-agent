# GGUF Projector Selection Incident

Date: `2026-10-17`. Affected release: `v1.1.2` at `0ea91c7`.
Status: closed `2026-10-18`; replacement release, Homebrew upgrade, managed
runtime refresh and original user workflow verified. Parent: [maintenance plan](./bugfix-maintenance-plan.md).

## User Impact

`tentgent model pull google/gemma-3-4b-it-qat-q4_0-gguf` completed, but the
following text chat failed before inference with HTTP 400. The reported
snapshot revision was `15f73f5eee9c28f53afefef5723e29680c2fc78a`; its source
contained `gemma-3-4b-it-q4_0.gguf` and `mmproj-model-f16-4B.gguf`.
The runtime counted both as candidate models and rejected the directory.
The download and model identity were not the cause.

The same resolver serves GGUF chat and embedding. Any source with one primary
GGUF and a recognized projector sidecar was subject to this selection failure.
This establishes the code-level scope; it is not a claim that every such
model supports either capability or that other users encountered the error.

## Verified Cause And History

- Initial commit `e5885b5` introduced `_resolve_gguf_path` in the former Python
  daemon with `glob("*.gguf")` and an exactly-one-file requirement. Its error
  explicitly described an MVP restriction. Commit `354d747` preserved this
  assumption in the shared llama.cpp resolver used by chat and embedding.
- The [released Python resolver](../../python/tentgent-model-runtime/src/tentgent/runtime/backends/llama_cpp/common.py)
  treated a GGUF extension as evidence that a file was a primary model.
  A multimodal projector also uses that extension, so file count was not
  equivalent to model count.
- Commit `9f1678c` (`#106` / PR `#111`) added
  [Rust file diagnostics](../../src/tentgent-kernel/src/features/model/file_diagnostics.rs)
  with the same assumption. Its suggested removal and re-pull could not
  repair a valid upstream layout and incorrectly suggested corruption.
- The [snapshot helper](../../python/tentgent-model-runtime/src/tentgent/runtime/tools/hf_snapshot.py)
  downloads the full resolved snapshot without file filters. Keeping both
  files is intentional. The [pull use case](../../src/tentgent-kernel/src/features/model/usecases/pull.rs)
  finalizes storage and metadata; a successful pull is not an inference test.

This was a long-standing product assumption exposed in `v1.1.2`, rather than
a selection regression introduced by the `#132` server-option changes.
Neither the user's command nor their retained projector file required repair.

## Why Validation Missed It

At released commit `0ea91c7`:

- The Python test directory contained no GGUF selection or llama.cpp loader
  test. Its only `GGUF` reference was in an unrelated asset-safety error test.
- Rust diagnostic tests covered a single `model.gguf` and a missing GGUF file,
  but no primary-model-plus-projector layout. Diagnostics therefore repeated
  the loader's assumption without independently challenging it.
- [Native source validation](../../scripts/test-release-source.sh) ran the
  existing suites. Repeating those suites across architectures could expose
  platform defects, but could not cover an absent repository-layout case.
- [Installed-artifact validation](../../scripts/test-installed-release.py)
  explicitly avoided model downloads and checked installation, version,
  source location and imports. Those checks did not exercise model selection.
- The recorded [real-model release evidence](./issue-132-validation-evidence.md#real-model-and-resource-evidence)
  focused on MLX load, idle release, restart and cleanup. It did not establish
  llama.cpp behavior for a full Gemma GGUF snapshot. The
  [fixture list](../user/model-fixtures.md#current-fixture-models) also had no
  GGUF-plus-projector fixture.

The engineering failure was carrying an MVP simplification forward without
an explicit asset-role contract and a representative fixture. Our previous
validation emphasized lifecycle, packaging and platform correctness; those
were necessary checks, but the wrong evidence for claiming this GGUF workflow
worked. Large passing-test counts do not compensate for that missing case.
The evidence above establishes the coverage gap; it cannot establish every
individual reviewer's reasoning or any unrecorded manual testing.

## Remediation

The [selection contract](../contracts/model-store.md#gguf-runtime-selection)
now defines one primary GGUF at the source root. Both implementations exclude
case-insensitive stems `mmproj`, `mmproj-...` and `mmproj_...`, accept
case-insensitive GGUF extensions, and consider files rather than directories.
They reject projector-only sources, directly selected projectors and multiple
primary models. Projector files remain in the store and manifest, preserving
model identity. This is a filename-convention fix for text execution; it does
not enable vision or parse arbitrary GGUF asset roles.

[Python regression cases](../../python/tentgent-model-runtime/tests/test_gguf_selection.py)
exercise selection and both chat/embedding loader wiring.
[Rust regression cases](../../src/tentgent-kernel/src/features/model/tests/gguf_diagnostics.rs)
cover matching diagnostics. They include the reported filenames, case variants,
projector-only inputs, ambiguous primary models, prefix boundaries and nested
or directory-shaped candidates. The failure guidance now describes a usable
source layout instead of recommending an ineffective re-download.

## Initial Repair Evidence

The repair session recorded 192 passing Python tests with 15 optional tests
skipped in the Python 3.12 base environment, and 10 passing related Rust tests.
The downloaded Gemma snapshot returned HTTP 200 for non-streaming and SSE
chat with `llama-cpp-python` 0.3.23 and the corrected runtime source selected
explicitly. Both GGUF files remained in place. These results validate the
source repair on the reported model; the source override is not evidence that
the published release or Homebrew installation already contains the fix.
The release evidence below supersedes these initial counts.

The local installation audit also found CLI `1.1.2` alongside a non-editable
Python package still installed from the `1.1.1` Cellar. The resolver defect
exists in both releases, but this establishes a separate upgrade gap:
Homebrew updates packaged files without syncing the managed Python runtime.
This required verification of bootstrap refresh and a fresh worker process
in addition to the CLI version and formula revision.

## Release Acceptance And Follow-Up

The user explicitly requested replacement of the existing `v1.1.2` release
once tests pass, followed by a working Homebrew upgrade. This is an authorized
same-version replacement, not a new semantic-version release. Preserve the
old release commit and asset checksums in the release evidence so the two
builds can be distinguished and the replacement can be audited.

### Replacement Release Evidence (`2026-10-18`)

[PR #143](https://github.com/HiroLiang/tentserv-agent/pull/143) merged at
`a850a3f`; the authorized replacement `v1.1.2` tag points to that commit.
The original source remains identified by
`archive/v1.1.2-before-gguf-20261017` at `0ea91c7`, and the previous release
archives and metadata were retained before replacement.

- [x] Final native source, packaging and installed-artifact gates passed on
  macOS ARM/Intel, Linux x86_64 and Windows x86_64 in
  [release run 39017736205](https://github.com/HiroLiang/tentserv-agent/actions/runs/39017736205).
  The installed probes exercised primary-plus-projector selection from
  non-editable site-packages on all four platforms.
- [x] The local Python suite with optional dependencies passed all 207 tests
  without skips. Native base environments passed 192 tests with 15 optional
  dependency skips on macOS/Linux; Windows passed 190 with those 15 skips
  plus two POSIX-only FIFO skips. Each native suite also passed 11 subtests.
  These skips are recorded as exclusions, not successful native coverage.
- [x] Both macOS notarizations were `Accepted`: ARM submission
  `7089978b-88cd-496f-95a9-e39b53743217`; Intel submission
  `c3220a56-edcd-4971-903a-19bf5688e81a`. All four downloaded replacement
  archives matched the published checksums.
- [x] The published signed ARM archive was installed with a non-editable
  Python 3.12.13 runtime and `llama-cpp-python` 0.3.23. Its actual CLI completed
  both ordinary and streaming Gemma chat, using the reported snapshot with
  both GGUF files retained. Installed module hashes matched the packaged
  source. The isolated worker exited, and production model data was unchanged.
- [x] [Tap commit 1287ddb](https://github.com/HiroLiang/homebrew-tap/commit/1287ddb)
  published formula revision 1 with replacement checksums and cache-distinct
  URLs. `brew update` and actual upgrade from `1.1.2` to `1.1.2_1` succeeded.
  Bootstrap from the new Cellar refreshed the managed environment to Python
  3.12.13 with 150 locked packages. Non-editable package provenance and
  installed resolver/chat hashes matched the new Cellar source.
- [x] The upgraded CLI and managed environment passed ordinary and streaming
  Gemma chat with a fresh worker, followed by confirmed worker exit. The
  isolated verification retained both GGUF files and left production data
  unchanged. The user's short model ref also succeeded through default
  installed paths outside the checkout, without `TENTGENT_*` or Python source
  overrides, for both ordinary and streaming chat. `doctor` reported `ready`
  and `cpu-gguf` reported `Ready`.
- [x] `brew audit --formula hiroliang/tap/tentgent` and
  `brew test hiroliang/tap/tentgent` both exited successfully. The formula test
  executed the installed `1.1.2_1` binary; no checks were skipped or bypassed.

The asset names remain `tentgent-1.1.2-<target>.tar.gz` (`.zip` for Windows).
The retained hashes distinguish the two same-version builds:

| Target | Original SHA-256 | Replacement SHA-256 |
| --- | --- | --- |
| `aarch64-apple-darwin` | `956cbb7b277654385ad5dac8d5fd1dd4b16802c95caafb83ddf7827bd97908af` | `0704b9e9192a3f4eff057a281087c2e9571df1b50681e25fb6e9d15457043b51` |
| `x86_64-apple-darwin` | `74cb92ec07b1d412f7f894bf385e43d86874006048f4d60ca8bf419082d8614c` | `0f25a865e682009dc7a754afb7f615c338b16ae42868201ab087d6155e4d7554` |
| `x86_64-pc-windows-msvc` | `da4b1c90db4149a2a82dd06fd77a62e97dcfb6af2dbf7e472a6332d2216f6542` | `0b1aa347058d01a8806d0c80957e254e26e5acc50e63297d9a8cb8edc81b4e20` |
| `x86_64-unknown-linux-gnu` | `b02c75a4ec4e6a2b55ffdd4217e05b769e11c9009ecfc5e130e78ef03b20264e` | `225f3ed3a53098b43bdac31c6ab0385d110f98fc0da7e35fa09d4c05ee9dad38` |

All incident acceptance gates passed. Future changes to asset selection must review both Rust diagnostics and Python
execution against the same layout matrix. Backend release evidence must name
the tested backend and layout; another backend's smoke cannot establish it.
