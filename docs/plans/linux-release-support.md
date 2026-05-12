# Linux Release Support

This plan adds first-class Linux release and install support after the 0.3.x
Homebrew tap path has stabilized.

## Current State

- GitHub Release assets are built for macOS Apple Silicon, macOS Intel, and
  Windows x86_64 only.
- `scripts/install.sh` detects only Darwin targets and rejects Linux hosts.
- `scripts/package-local.sh` does not package Linux targets.
- Python bootstrap scripts already know Linux `uv` target names, but the full
  release/install path has not been smoke-tested on Linux.
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

- Add `x86_64-unknown-linux-gnu` to the release workflow matrix on an Ubuntu
  runner.
- Include the Linux tarball in release assets and `checksums.txt`.
- Update release notes asset lists and install docs.
- Keep Homebrew tap helper unchanged; it should continue updating only macOS
  formula URLs and checksums.

Review target:

- A stable tag publishes a Linux tarball beside the existing macOS and Windows
  assets.

### L3: Linux Installer Smoke

- Add CI or documented manual smoke for:
  - `install.sh --skip-python-bootstrap --skip-doctor`
  - installed `tentgent -V`
  - `tentgent runtime bootstrap --print-plan`
  - `tentgent doctor`
- Verify the installed CLI resolves packaged Python support files from the
  install prefix.
- Verify Linux runtime home defaults and override behavior through
  `TENTGENT_HOME`.

Review target:

- Linux install works without cloning the repository and without mutating
  source-tree paths.

### L4: Full Bootstrap Smoke

- Run a full `tentgent runtime bootstrap` smoke in a temporary `TENTGENT_HOME`
  on Linux.
- Record expected bootstrap cache and managed Python env paths.
- Treat backend dependency warnings as acceptable when they are capability
  warnings, not install failures.

Review target:

- A Linux install can prepare the managed Python runtime and pass doctor with
  only expected backend warnings.

### L5: Optional Linux Expansion

- Evaluate `aarch64-unknown-linux-gnu` after x86_64 is stable.
- Decide whether Linux package-manager channels are worth adding.
- Revisit glibc compatibility and minimum supported distro after real smoke
  data exists.

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
