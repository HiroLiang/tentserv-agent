# Packaging And Install MVP

This plan defines the installation and release track for moving Tentgent from source-first development to user-friendly installs.

## Scope

- Define the first release artifact shape.
- Support a `curl` installer first.
- Keep Homebrew as the first polished package-manager path.
- Keep the Rust CLI as the primary user-facing entry point.
- Preserve a clean Rust-to-Python runtime boundary.

## Goals

- Let users install Tentgent without cloning the repository.
- Avoid source-tree assumptions such as `python/tentgent-daemon/.venv`.
- Do not require end users to preinstall `uv` or other developer bootstrap tools.
- Use one release artifact shape for manual, `curl`, and Homebrew installs.
- Keep checksums and versioned artifacts mandatory from the first installer slice.

## Non-Goals

- Do not submit to `homebrew/core` in this track.
- Do not implement macOS notarization in the first slice.
- Do not package model weights, adapters, datasets, or `TENTGENT_HOME`.
- Do not auto-install provider API keys or user secrets.
- Do not add self-update behavior to the CLI.

## Release Artifact Shape

First release artifacts should be versioned archives:

```text
tentgent-<version>-<target>.tar.gz
tentgent-<version>-<target>.zip
checksums.txt
```

Initial targets:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-pc-windows-msvc`

Planned later targets:

- `aarch64-pc-windows-msvc`
- Linux targets after dependency packaging is clarified

Each archive should contain:

```text
bin/tentgent or bin/tentgent.exe
share/tentgent/python/
LICENSE
README.md
```

`share/tentgent/python/` may initially contain the Python project source and metadata rather than a prebuilt environment.
This artifact shape is a smoke-test target until Python environment bootstrap no longer depends on a user-installed `uv`.

## Runtime Layout

Installed runtime state must stay separate from the install prefix.

Suggested install prefix:

```text
~/.local/bin/tentgent
~/.local/share/tentgent/
```

Suggested runtime home remains platform-managed or `TENTGENT_HOME`:

```text
~/Library/Application Support/com.tentserv.tentgent/
```

The installed CLI must resolve Python helpers from the installed prefix, not from the repository source tree.

## Python Runtime Strategy

Track A is the first implementation target:

- install the Rust CLI
- install Python daemon source under `share/tentgent/python`
- create or reuse a managed Python environment under Tentgent-owned support data
- invoke daemon entry points through that managed environment

The current `doctor --fix` implementation uses `uv` only as a developer bootstrap. A publishable installer must either bundle the bootstrap tool, download a pinned bootstrap tool, use a prebuilt Python environment artifact, or replace this step with another user-owned runtime strategy.

Track B remains a future option:

- package Python daemon entry points as executables
- have Rust invoke stable daemon binaries instead of Python modules

Do not bundle local model runtimes or downloaded model files into release artifacts.

## Installer Channels

### Curl Installer

Desired user command:

```text
curl -fsSL https://agent.tentserv.com/install.sh | sh
```

The installer must:

- detect OS and CPU architecture
- download a versioned release tarball
- verify `sha256` from `checksums.txt`
- install `tentgent` into a user-writable bin directory
- install support files under a user-writable share directory
- initialize or check the Python daemon environment
- print `tentgent doctor` as the next verification command

### PowerShell Installer

Desired user command:

```text
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

The installer must:

- download the versioned Windows `.zip` artifact
- verify `sha256` from `checksums.txt`
- install `tentgent.exe` into a user-writable bin directory
- install support files under a user-writable share directory
- initialize or check the Python daemon environment
- avoid editing the user's PowerShell profile automatically

### Homebrew Tap

Desired user command:

```text
brew tap tentserv/tap
brew install tentgent
```

Use a project-owned tap first, not `homebrew/core`, because Tentgent may ship binary artifacts and should prove packaging stability before a public tap submission.

The formula should:

- point to a versioned GitHub Release tarball
- include `sha256`
- install `bin/tentgent`
- install support files into `pkgshare`
- define a small `test do` block such as `tentgent --version`

## Implementation Slices

### Slice 1: Runtime Resolver Contract

- Status: implemented in the active workspace.
- define installed-prefix lookup for Python daemon assets
- preserve source-tree fallback for local development
- document environment overrides for debugging
- expose `tentgent status` as the current diagnostic command until `doctor` exists

Review target:

- no release scripts yet
- CLI can explain where it expects Python runtime assets

### Slice 2: Doctor Command

- Status: implemented in the active workspace.
- add `tentgent doctor`
- check CLI version, runtime home, Python daemon availability, and key directories
- avoid network checks by default
- report missing Python runtime setup with actionable commands

Review target:

- users can verify install health after `curl` or Homebrew installation

### Slice 3: Local Release Script

- Status: implemented in the active workspace.
- add a script that builds `cargo build --release`
- package `bin/tentgent`, Python source, `README.md`, and `LICENSE`
- produce `.tar.gz` and `checksums.txt`
- keep model/test/runtime data excluded

Review target:

- one local command creates a release-like smoke-test tarball

### Slice 3.5: Python Env Bootstrap

- Status: implemented in the active workspace as a developer bootstrap only.
- add `tentgent doctor --fix`
- use the resolved Python project and `UV_PROJECT_ENVIRONMENT`
- run `uv --no-config sync --project <resolved-python-project>`
- keep regular `doctor` read-only

Review target:

- developers can smoke-test installed-prefix packages before the release-ready bootstrap exists

Release gate:

- this slice is not sufficient for public release because it requires `uv` on the user's PATH
- no public installer should be published until this requirement is removed or owned by the installer

### Slice 3.6: User Bootstrap Strategy

- Status: implemented in the active workspace.
- choose the release bootstrap strategy before writing the public `install.sh`
- use downloaded pinned `uv` as the first MVP strategy
- require zero preinstalled `uv` expectation for normal users
- keep prebuilt Python env artifacts and daemon executables as later alternatives
- document cache/runtime locations and failure recovery behavior

Review target:

- the selected installer path can create a working Python runtime from a clean user machine without developer tools

Decision:

- The first public installer should download a pinned `uv` executable into a Tentgent-owned bootstrap cache, verify it, use it to create the managed Python environment, then run Tentgent through the managed entry points.
- The downloaded `uv` is an installer/bootstrap implementation detail, not a runtime requirement and not expected on the user's PATH.

### Slice 3.7: Bootstrap Cache Contract

- Status: implemented in the active workspace.
- define `TENTGENT_HOME/runtime/bootstrap/` as the default cache for installer-owned bootstrap tools
- store downloaded bootstrap tools under versioned, platform-specific paths
- store a checksum or manifest for each downloaded tool
- make `tentgent doctor` report whether the bootstrap cache exists without requiring it for normal runtime commands
- keep `TENTGENT_PYTHON_ENV_DIR` as the override for the managed Python environment

Review target:

- runtime-home docs explain where installer-owned bootstrap tools live and how they differ from runtime entry points

### Slice 3.8: Pinned `uv` Downloader

- Status: implemented in the active workspace.
- add installer logic that resolves OS and architecture to a pinned `uv` artifact
- download into a temporary file under the bootstrap cache
- verify checksum before making the executable available
- avoid using `uv` from the user's PATH unless an explicit developer flag requests it
- fail with a concise recovery message when offline, unsupported, or checksum verification fails

Review target:

- a clean machine without `uv` can acquire the bootstrap tool through the installer path

Implementation note:

- `scripts/bootstrap-uv.sh` pins `uv 0.11.7`
- it supports `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, and `x86_64-unknown-linux-gnu`
- it verifies the pinned upstream `sha256.sum` file before trusting per-asset checksums from that file
- it writes to `TENTGENT_HOME/runtime/bootstrap/uv/<version>/<target>/`

### Slice 3.9: Installer Python Env Sync

- Status: implemented in the active workspace.
- use the cached pinned `uv` executable to run `uv --no-config sync --project <installed-python-project>`
- set `UV_PROJECT_ENVIRONMENT=<TENTGENT_HOME>/runtime/python-env`
- verify required entry points after sync
- print `tentgent doctor` as the final verification command

Review target:

- after installation, `tentgent chat`, `tentgent server`, `tentgent train`, and HF pull helpers use managed env entry points without invoking `uv`

Implementation note:

- `scripts/bootstrap-python-env.sh` resolves the packaged or development Python project
- it defaults the environment to `TENTGENT_HOME/runtime/python-env`
- it ensures pinned `uv` is available through `scripts/bootstrap-uv.sh`
- it keeps `UV_CACHE_DIR` under `TENTGENT_HOME/runtime/bootstrap/uv-cache`
- it requests managed Python `3.13`, frozen lockfile sync, and non-editable package installation
- `scripts/package-local.sh` now includes both bootstrap scripts under `share/tentgent/scripts/`

### Slice 3.10: Runtime No-`uv` Guard

- Status: implemented in the active workspace.
- ensure normal runtime commands never fall back to `uv` in installed-prefix mode
- keep `uv` fallback only for development-source mode or an explicit developer override
- make missing entry points produce an installer/bootstrap repair hint instead of trying user PATH tools

Review target:

- installed-prefix runtime behavior is deterministic and does not depend on developer tools

Implementation note:

- HF snapshot helpers now use managed `tentgent-hf-snapshot` entry points in installed-prefix mode
- if the installed-prefix entry point is missing, model/adapter pull returns a bootstrap repair error instead of invoking `uv`
- CLI runtime entrypoint errors now differentiate installed-prefix repair from development `doctor --fix`

### Slice 4: Curl Installer Draft

- depends on Slices 3.6 through 3.10
- Status: implemented in the active workspace.
- add `scripts/install.sh`
- install from a local or GitHub Release URL
- verify checksums
- support `--prefix` or env-based install destination
- do not require sudo for the default path

Review target:

- install into a temporary prefix and run `tentgent doctor`

Implementation note:

- `scripts/install.sh` supports local paths, `file://` URLs, and HTTPS URLs
- it verifies the archive against `checksums.txt`
- it installs `bin/tentgent`, `share/tentgent/python`, and `share/tentgent/scripts`
- by default it runs `bootstrap-python-env.sh` and then `tentgent doctor`
- `--skip-python-bootstrap --skip-doctor` exists for layout smoke tests that should not download heavy ML dependencies

### Slice 5: GitHub Release Workflow

- Status: implemented in the active workspace.
- add GitHub Actions build matrix for macOS and Windows x86_64 targets
- upload artifacts and checksums to a draft release
- keep signing/notarization out of this slice

Review target:

- a tagged version can produce release artifacts reproducibly

Implementation note:

- `.github/workflows/release.yml` runs on `v*.*.*` tag pushes and manual dispatch with an existing tag
- it builds native macOS artifacts on `macos-14` for Apple Silicon and `macos-15-intel` for Intel
- it builds a native Windows x86_64 artifact on `windows-latest`
- each package job runs `scripts/package-local.sh` with `TENTGENT_VERSION` and `TENTGENT_TARGET`
- the release job merges per-target checksum files into one `checksums.txt`
- release assets include macOS tarballs, the Windows zip, `checksums.txt`, `install.sh`, and `install.ps1`
- the release copy of `install.sh` is rewritten with the tag version and GitHub Release asset URL so `latest/download/install.sh | sh` works without extra environment variables
- the release copy of `install.ps1` is rewritten with the tag version and GitHub Release asset URL so `latest/download/install.ps1 | iex` works without extra environment variables
- release assets are uploaded to a draft GitHub Release through `gh release`
- signing and notarization remain deferred to Slice 7

### Slice 5.5: Windows Installer And Runtime Bootstrap

- Status: implemented in the active workspace.
- add `scripts/install.ps1`
- install Windows `.zip` artifacts into `%LOCALAPPDATA%\Programs\tentgent`
- use `%LOCALAPPDATA%\tentserv\tentgent\data` as the installer-side equivalent of Rust's default Windows runtime home
- download pinned `uv.exe` into Tentgent's bootstrap cache
- create `TENTGENT_HOME/runtime/python-env` with uv-managed Python
- verify Windows `.exe` entry points after sync
- keep PowerShell profile and PATH edits manual
- gate MLX dependencies to Apple Silicon macOS so Windows bootstrap can resolve PEFT/safetensors dependencies

Review target:

- a Windows x86_64 GitHub Actions runner can produce a zip artifact and draft release asset
- a Windows user can install with `install.ps1` and run `tentgent doctor`

### Slice 6: Homebrew Tap Formula

- Status: planned.
- create or document `homebrew-tap`
- add `Formula/tentgent.rb`
- use the same release artifact and checksum
- add formula test

Review target:

- `brew install tentgent` works from the tap

### Slice 7: Signing And Notarization

- Status: planned.
- keep this as a release engineering/security slice, not part of the TUI
  implementation track
- sign macOS binaries with a stable Developer ID Application identity and
  `com.tentserv.tentgent` signing identifier
- use `--options runtime` and `--timestamp` for distribution-signed macOS
  command-line binaries
- verify the signed binary with `codesign --verify --strict --verbose=4` and
  record the designated requirement with `codesign -dr -`
- package macOS releases as signed `.pkg` or another artifact shape that can be
  notarized and stapled; do not rely on stapling standalone Mach-O binaries
- submit release artifacts to Apple notarization through `notarytool` using
  credentials stored outside the repository
- staple and validate distributable artifacts when the artifact type supports it
- document development signing separately from release signing, including the
  warning that `cargo run` may rebuild and replace a just-signed binary
- document Keychain prompt behavior after signing and how stable designated
  requirements reduce repeated `Always Allow` prompts
- keep private signing keys, `.p12` files, App Store Connect API keys, and
  notarization credentials out of the repository and out of plain-text
  environment variables
- restrict release signing to protected tags/branches and never expose signing
  secrets to untrusted pull requests

Review target:

- macOS release artifacts pass Gatekeeper checks, have a stable code identity
  for Keychain ACLs, and can be verified without exposing signing credentials

## Open Questions

- Should the first release artifacts be GitHub-only or also mirrored at `agent.tentserv.com`?
- Should `install.sh` default to latest stable or require an explicit version?
- Should Python dependencies be installed during installation, first run, or `tentgent doctor --fix`?
- Should heavyweight dependencies such as `torch`, `mlx-lm`, and `llama-cpp-python` be optional feature bundles later?
- Should Windows use a PowerShell installer first, or wait until the Python dependency bootstrap is stable on macOS?
- Should the release bootstrap bundle pinned `uv`, download pinned `uv`, ship a prebuilt Python environment artifact, or ship Python daemon executables?
