# Linux Release Support

This plan adds first-class Linux release and install support after the 0.3.x
Homebrew tap path has stabilized.

## Current State

- Existing published GitHub Release assets before the first L2 tag were built
  for macOS Apple Silicon, macOS Intel, and Windows x86_64 only.
- `scripts/install.sh` and `scripts/package-local.sh` know the Linux x86_64
  target name after L1.
- The release workflow knows the Linux x86_64 package job after L2, and
  `v0.3.4-alpha.1` published the first Linux x86_64 asset.
- Python bootstrap scripts already know Linux `uv` target names. L4 split the
  managed Python runtime into profiles, and L5 verified the default base
  profile against the `v0.3.4-alpha.2` GitHub Release.
- Homebrew remains a macOS distribution path; Linux support should start with
  GitHub Release tarballs and `install.sh`, not Linuxbrew.

## Goals

- Publish a Linux x86_64 release archive:
  `tentgent-<version>-x86_64-unknown-linux-gnu.tar.gz`.
- Make `install.sh` install that archive on Linux x86_64 with checksum
  verification.
- Keep runtime state under Linux platform defaults or `TENTGENT_HOME`, separate
  from the install prefix.
- Verify at least `tentgent -V`, `tentgent runtime bootstrap --print-plan`, and
  `tentgent doctor` on Linux.
- Document Linux as supported for CLI/install smoke before claiming full local
  model-runtime parity.

## Non-Goals

- Do not add Linuxbrew support in the first Linux slice.
- Do not add distro packages such as `.deb`, `.rpm`, or AUR packages.
- Do not promise GPU acceleration, CUDA setup, or local ML backend parity in
  the first release.
- Do not cross-compile Linux from macOS; use native GitHub Actions Linux
  runners first.

## Slices

### L1: Local Linux Target Plumbing

- Status: implemented.
- Add Linux target detection to `scripts/package-local.sh` and `scripts/install.sh`.
- Support `x86_64-unknown-linux-gnu` only for Linux in this slice.
- Add explicit installer and package target allowlists so unsupported explicit
  targets fail before archive names or release URLs are built.
- Add `scripts/package-local.sh --print-plan` for hermetic target mapping
  checks that do not build, create `dist/`, or require a native matching host.
- Add `scripts/test-linux-release-targets.sh` for Linux dry-run mapping,
  unsupported target failures, hermetic `--print-plan`, and native-host gate
  behavior.

Review target:

- A Linux host can package or install a Linux tarball by target name without
  source-tree path assumptions.

### L2: GitHub Release Linux Asset

- Status: implemented.
- Prerequisite: L1 is included in the release tag, so `install.sh`,
  `package-local.sh`, and `scripts/test-linux-release-targets.sh` already know
  `x86_64-unknown-linux-gnu`.
- Add `x86_64-unknown-linux-gnu` to the release workflow matrix on an Ubuntu
  runner.
- Include the Linux tarball in release assets and `checksums.txt`.
- Update generated release notes asset lists and smoke snippets.
- Keep Homebrew tap helper unchanged; it should continue updating only macOS
  formula URLs and checksums.

Execution details:

- Add a Linux matrix row to `.github/workflows/release.yml`:
  - `target: x86_64-unknown-linux-gnu`
  - `runner: ubuntu-24.04`
  - `archive_ext: tar.gz`
- Strengthen the workflow native-target verifier from architecture-only checks
  to `target:os:arch` checks:
  - `aarch64-apple-darwin:Darwin:arm64`
  - `x86_64-apple-darwin:Darwin:x86_64`
  - `x86_64-unknown-linux-gnu:Linux:x86_64`
  - `x86_64-pc-windows-msvc:MINGW*/MSYS*/CYGWIN*:x86_64`
- Keep package creation delegated to `scripts/package-local.sh`; L2 should not
  add a second packaging algorithm in workflow YAML.
- Keep the release job artifact collection generic:
  - tarballs are copied with `tentgent-*.tar.gz`
  - Windows zips are copied with `tentgent-*.zip`
  - per-target checksum files are concatenated into one `checksums.txt`
- Add release-job assertions after `dist-release` is prepared:
  - macOS Apple Silicon tarball exists
  - macOS Intel tarball exists
  - Linux x86_64 tarball exists
  - Windows x86_64 zip exists
  - `checksums.txt` contains exactly one package checksum entry for each
    expected artifact, and exactly four package checksum entries total
- Add the Linux tarball to generated release notes:
  `tentgent-<version>-x86_64-unknown-linux-gnu.tar.gz`.
- Add a Linux installer smoke snippet to generated release notes using
  `install.sh --skip-python-bootstrap --skip-doctor`.
- Add a release-notes caveat that Linux support is CLI/install archive
  availability only; full managed runtime and local backend parity are not
  claimed yet.
- Do not change `scripts/update-homebrew-formula.sh`; it should continue to
  extract only macOS checksums for the Homebrew formula even when
  `checksums.txt` contains Linux and Windows entries.
- Add a Linux checksum entry to the Homebrew helper fixture test and assert the
  formula still does not include Linux URLs.
- Do not add Linuxbrew, `.deb`, `.rpm`, or user-facing Linux package-manager
  instructions in this slice.

Implementation acceptance:

- A tag-triggered release workflow has four package jobs:
  macOS Apple Silicon, macOS Intel, Windows x86_64, and Linux x86_64.
- The Linux package job publishes
  `tentgent-<version>-x86_64-unknown-linux-gnu.tar.gz`.
- The merged release `checksums.txt` includes exactly one checksum entry for
  each expected release archive.
- GitHub release notes list the Linux asset and the Linux installer smoke.
- Homebrew formula update helper behavior remains macOS-only, and its tests
  pass with Linux checksum entries present.
- The release remains stable/prerelease-safe through the existing
  `scripts/release-metadata.sh` flags.
- Stable tags and prerelease tags publish the same package artifact set when
  the release workflow runs; Homebrew formula updates remain stable-tag-only.

Suggested verification commands before tagging:

```bash
bash -n scripts/package-local.sh
bash -n scripts/install.sh
bash -n scripts/release-metadata.sh
bash -n scripts/test-release-metadata.sh
bash -n scripts/update-homebrew-formula.sh
bash -n scripts/test-update-homebrew-formula.sh
bash -n scripts/test-linux-release-targets.sh
bash scripts/test-release-metadata.sh
bash scripts/test-update-homebrew-formula.sh
bash scripts/test-linux-release-targets.sh
TENTGENT_TARGET=x86_64-unknown-linux-gnu scripts/package-local.sh --print-plan
git diff --check
```

Suggested release verification after the first Linux tag:

```bash
gh release view v<version> --json tagName,isPrerelease,url
gh release download v<version> \
  --pattern 'tentgent-<version>-x86_64-unknown-linux-gnu.tar.gz' \
  --pattern checksums.txt \
  --dir /tmp/tentgent-linux-release-smoke
grep 'tentgent-<version>-x86_64-unknown-linux-gnu.tar.gz' \
  /tmp/tentgent-linux-release-smoke/checksums.txt
```

Review target:

- A stable tag publishes a Linux tarball beside the existing macOS and Windows
  assets.

### L3: Linux Installer Smoke

- Status: implemented against `v0.3.4-alpha.1`.
- Add CI or documented manual smoke for:
  - `install.sh --skip-python-bootstrap --skip-doctor`
  - installed `tentgent -V`
  - `tentgent runtime bootstrap --print-plan`
  - `tentgent doctor`
- Verify the installed CLI resolves packaged Python support files from the
  install prefix.
- Verify Linux runtime home defaults and override behavior through
  `TENTGENT_HOME`.
- Use `bash -s --` for piped Unix installer smoke on Linux. The installer is a
  Bash script, and Ubuntu `/bin/sh` is usually `dash`.
- Verified release asset checksum with `sha256sum -c` against
  `checksums.txt`.
- Verified both GitHub Release curl installer and downloaded direct archive
  install paths in Docker `ubuntu:24.04` on `linux/amd64`.
- Verified installed binary reports `tentgent 0.3.4-alpha.1`.
- Verified `runtime bootstrap --print-plan` points to:
  - installed project under `$PREFIX/share/tentgent/python`
  - managed env under `$TENTGENT_HOME/runtime/python-env`
  - bootstrap cache under `$TENTGENT_HOME/runtime/bootstrap`
- Verified `--print-plan` did not create Python env or pinned `uv` tool
  directories.
- Note: direct archive smoke still needs `curl` installed because the installer
  command helper requires it even for local archive/checksum paths.

Review target:

- Linux install works without cloning the repository and without mutating
  source-tree paths.

### L4: Runtime Dependency Profiles

- Status: implemented in source; release smoke moves to L5.
- Split the managed Python runtime into explicit dependency profiles before
  claiming stable Linux runtime readiness.
- Motivation from the `v0.3.4-alpha.1` full-bootstrap smoke:
  - Minimal `ubuntu:24.04` on `linux/amd64` with only `bash`, `curl`,
    `ca-certificates`, `coreutils`, `gzip`, and `tar` did not pass full
    bootstrap.
  - `llama-cpp-python==0.3.20` attempted a native build.
  - CMake failed because no C/C++ compiler was available:
    `Could not find the compiler specified in the environment variable CC:
    cc -pthread`.
  - A clean diagnostic rerun with `build-essential`, `cmake`, and `pkg-config`
    passed, but the runtime footprint was about 9.3 GiB.
- Defined a base profile that can bootstrap in a minimal Linux container without
  compilers or heavyweight ML wheels.
- Moved heavyweight local-model and training dependencies behind optional
  Python extras instead of installing them by default.
- Runtime bootstrap profiles:
  - `base`: common CLI Python entrypoints and lightweight runtime helpers
  - `local-model`: local model serving dependencies such as `llama-cpp-python`,
    Transformers, PEFT, Torch, and macOS arm64 MLX dependencies
  - `training`: LoRA/training dependencies such as Torch, Transformers, and PEFT
  - `full`: bootstrap alias for `local-model + training`
- Updated `tentgent runtime bootstrap` to select `base` by default and expose
  `--profile <base|local-model|training|full>`.
- Updated `scripts/bootstrap-python-env.sh --print-plan` to show runtime profile
  and uv extras without resolving or downloading pinned uv.
- Kept `doctor` as a base runtime health check; missing local-model or training
  packages remain backend/capability warnings instead of base install failures.
- Made heavyweight backend imports lazy enough that base entrypoint import/help
  paths do not fail with raw Python import tracebacks.
- Keep existing chat/session/server behavior unchanged for already bootstrapped
  environments.

Implementation verification:

```bash
bash scripts/test-runtime-profiles.sh
cargo test -p tentgent-cli runtime
cargo test -p tentgent-core doctor
```

- Verified a source-mounted minimal `ubuntu:24.04` / `linux/amd64` container can
  run the default base bootstrap without `build-essential`, `cmake`, or
  `pkg-config`; the resulting env installed 29 packages and did not include
  Torch, PEFT, llama-cpp-python, Transformers, MLX, or mlx-lm.

Review target:

- A default Linux runtime bootstrap is small enough and dependency-light enough
  to pass in a minimal container without build tools.

### L5: Linux Release Runtime Smoke

- Status: implemented against `v0.3.4-alpha.2`.
- Verified the release tag contains L4 commit `3faa329`.
- Verified GitHub Release metadata:
  - tag: `v0.3.4-alpha.2`
  - release type: prerelease
  - asset: `tentgent-0.3.4-alpha.2-x86_64-unknown-linux-gnu.tar.gz`
- Verified the Linux tarball checksum with `sha256sum -c` against the release
  `checksums.txt` in Docker `ubuntu:24.04` on `linux/amd64`.
- Extracted the release tarball and verified packaged support files include the
  L4 profile changes:
  - `scripts/bootstrap-python-env.sh` supports `--profile`
  - `--print-plan` prints `runtime profile:`
  - `python/pyproject.toml` contains `[project.optional-dependencies]`
  - `local-model` and `training` profile names are present
- Ran the primary release smoke in Docker `ubuntu:24.04` on `linux/amd64` with
  only `bash`, `curl`, `ca-certificates`, `coreutils`, `gzip`, and `tar`
  installed.
- Asserted no build tools were available before bootstrap:
  `cc`, `gcc`, `g++`, and `cmake` were all absent.
- Installed the release with:
  `install.sh --prefix <temp> --skip-python-bootstrap --skip-doctor`.
- Verified the installer and profile `--print-plan` paths do not create the
  managed Python env or pinned uv directory before bootstrap.
- Verified release profile planning:
  - `base` -> `uv extras: none`
  - `local-model` -> `uv extras: local-model`
  - `training` -> `uv extras: training`
  - `full` -> `uv extras: local-model, training`
  - unsupported profile names fail before invoking the shell bootstrap script
- Ran default `tentgent runtime bootstrap` twice; the second run was
  idempotent and checked the same 29 packages without reinstalling.
- Verified runtime outputs:
  - installed CLI reports `tentgent 0.3.4-alpha.2`
  - managed Python is `3.13.13`
  - pinned uv is `0.11.7`
  - uv package cache is non-empty
  - base env has 29 Python distributions
  - `torch`, `peft`, `llama_cpp`, `transformers`, `mlx`, and `mlx_lm` are not
    importable from the base env
  - `tentgent-hf-snapshot --help`, `tentgent-server --help`, and
    `tentgent-train-lora-run --help` do not traceback in the base env
  - `tentgent doctor` exits 0 and reports ready with expected backend
    capability warnings
- Recorded runtime footprint from the passing smoke:
  - `du -sh runtime/python-env`: about `94M`
  - `du -sh runtime/bootstrap`: about `149M`
  - `du -sh TENTGENT_HOME`: about `155M`
  - CLI doctor's human-readable size scan reported `86.9 MiB` for the Python
    env, `141.1 MiB` for bootstrap, and `228.0 MiB` for runtime home
- Verified default Linux runtime home planning without `TENTGENT_HOME`:
  `$HOME/.local/share/tentgent/runtime/python-env`.
- Note: do not use uv-cache filename matching as a heavy-dependency gate. The
  cache can contain this project source files such as `llama_cpp.py` or
  `peft_*.py`, and lightweight dependencies may ship compatibility files such
  as `_torch.py`; installed distributions and import specs are the reliable
  base-profile checks.
- Additional `local-model`, `training`, and `full` profile bootstraps remain
  diagnostics until those dependency contracts are made explicit.

Review target:

- A Linux install can prepare the managed Python runtime and pass doctor with
  only expected backend warnings.

### L6: Linux Preview User Docs And Readiness Boundary

- Status: implemented.
- Documented Linux x86_64 as a prerelease preview install path, not stable
  `latest` support.
- Added user-facing install guidance that uses the explicit
  `v0.3.4-alpha.2` release URL and `bash`, not `sh`, for the Unix installer.
- Documented that the Linux preview was smoke-tested on `ubuntu:24.04` /
  `linux/amd64`.
- Documented that the default `base` runtime bootstrap does not require
  `cc`, `gcc`, `g++`, or `cmake`.
- Documented Linux runtime-home default planning:
  `$HOME/.local/share/tentgent`.
- Documented the support boundary:
  - supported preview: x86_64 GitHub Release tarball install plus base runtime
    bootstrap and `doctor`
  - not claimed yet: local-model profile readiness, training profile readiness,
    GPU/CUDA, Linuxbrew, `.deb`, `.rpm`, AUR, and Linux arm64
- Updated user version notes so `v0.3.4-alpha.2` explains the Linux preview
  without moving the stable 0.3.x line.

Review target:

- A Linux user can find the correct preview install command and understand the
  current support boundary without reading the implementation plan.

### L7: Optional Linux Expansion

- Prerequisite: wire kernel runtime layout and capability state into runtime
  adapters and backend-gated workflow bundles in
  [tentgent-kernel-migration.md](./tentgent-kernel-migration.md) before
  advertising profile-specific Linux readiness.
- Evaluate `aarch64-unknown-linux-gnu` after x86_64 preview usage is stable.
- Decide whether Linux package-manager channels are worth adding.
- Revisit glibc compatibility and minimum supported distro after more smoke
  data exists.
- Define separate smokes for `local-model`, `training`, and `full` profiles
  before advertising local backend parity on Linux.
- Use manifest-backed probes to decide CPU vs GPU backend availability instead
  of inferring readiness only from OS or architecture.

Review target:

- Linux support has a clear next target without blocking x86_64 availability.

## Risks And Notes

- Native Rust binaries built on GitHub-hosted Ubuntu inherit glibc compatibility
  constraints from that runner image.
- Python ML dependencies may be larger and more backend-sensitive on Linux than
  on macOS; first support should separate install health from GPU/backend
  acceleration.
- `bootstrap-uv.sh` already maps Linux `uv` targets, but that is not sufficient
  proof that the full managed Python runtime works on every distribution.

## Verification Commands

```bash
bash -n scripts/package-local.sh
bash -n scripts/install.sh
cargo test --workspace
cargo fmt --check
git diff --check
```

Linux smoke examples:

```bash
scripts/install.sh \
  --archive dist/tentgent-<version>-x86_64-unknown-linux-gnu.tar.gz \
  --checksums dist/checksums.txt \
  --prefix /tmp/tentgent-linux-smoke \
  --skip-python-bootstrap \
  --skip-doctor

/tmp/tentgent-linux-smoke/bin/tentgent -V
TENTGENT_HOME="$(mktemp -d)" /tmp/tentgent-linux-smoke/bin/tentgent runtime bootstrap --print-plan
```
