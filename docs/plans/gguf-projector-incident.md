# GGUF Projector Selection Incident

Date: `2026-10-17`. Affected release: `v1.1.2` at `0ea91c7`.
Status: source remediation prepared; replacement-release and Homebrew
verification pending. Parent: [maintenance plan](./bugfix-maintenance-plan.md).

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
The final release matrix must supersede these initial counts.

The local installation audit also found CLI `1.1.2` alongside a non-editable
Python package still installed from the `1.1.1` Cellar. The resolver defect
exists in both releases, but this establishes a separate upgrade gap:
Homebrew updates packaged files without syncing the managed Python runtime.
Closeout must verify bootstrap refresh and a fresh worker process, not only
the CLI version or formula revision.

## Release Acceptance And Follow-Up

The user explicitly requested replacement of the existing `v1.1.2` release
once tests pass, followed by a working Homebrew upgrade. This is an authorized
same-version replacement, not a new semantic-version release. Preserve the
old release commit and asset checksums in the release evidence so the two
builds can be distinguished and the replacement can be audited.

Required evidence before closeout:

- [ ] Revalidate focused and full regressions against the final source; record
  skips and their reasons separately from successful coverage.
- [ ] Require an offline primary-plus-projector fixture through the installed
  runtime, so native packaging checks verify this fix without downloading a
  model or relying on development imports.
- [ ] Run real text chat and streaming with the reported Gemma snapshot through
  the candidate installation, retaining both GGUF files.
- [ ] Complete native release checks, signature/notarization requirements,
  replacement asset checksums and published-artifact verification.
- [ ] Publish a Homebrew formula revision with matching replacement checksums;
  demonstrate upgrade from the already installed `1.1.2` and repeat Gemma chat
  through the upgraded installation.

Local source smoke results and a published, upgradeable artifact are different
evidence. Keep this incident open until the replacement release and actual
Homebrew upgrade have been verified. Future changes to asset selection must
review both Rust diagnostics and Python execution against the same layout
matrix; future backend release evidence must name the tested backend and
layout instead of implying coverage from another backend's successful smoke.
