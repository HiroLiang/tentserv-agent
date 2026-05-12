# Install And Upgrade

Use this document for user-facing install and upgrade flows.

## Latest Install On macOS

Install through the project Homebrew tap:

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
tentgent --version
```

Homebrew installs the CLI and support files only. `tentgent runtime bootstrap`
creates or syncs the managed Python runtime under `TENTGENT_HOME`.

If you previously installed with `install.sh`, the old
`~/.local/bin/tentgent` may shadow the Homebrew binary on `PATH`. Check the
Homebrew build directly with:

```bash
/opt/homebrew/opt/tentgent/bin/tentgent -V
```

## Direct GitHub Release Installer On macOS

Use the direct installer when you want a script-based install or pinned release
artifact. The direct installer runs Python bootstrap by default:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
tentgent doctor
```

## Latest Install On Windows

Install the latest GitHub Release from PowerShell:

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

Temporarily add the default install location to `PATH`:

```powershell
$env:Path = "$env:LOCALAPPDATA\Programs\tentgent\bin;$env:Path"
tentgent doctor
```

The installer does not edit the user's PowerShell profile automatically.

## Pinned Install

Use a fixed direct-installer version when you want reproducible installation:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.ps1 | iex
```

The pinned installer is tied to that release's artifact URL and version.
`v0.3.3` is the current stable 0.3.x release; use `v0.2.0` if you want the previous
daemon-parity baseline.

## Upgrade

Upgrade Homebrew installs with:

```bash
brew update
brew upgrade hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
tentgent --version
```

Upgrade direct installer installs by running the installer again:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
tentgent doctor
tentgent --version
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
tentgent doctor
tentgent --version
```

The Homebrew formula updates:

- `/opt/homebrew/Cellar/tentgent/<version>/bin/tentgent`
- `/opt/homebrew/Cellar/tentgent/<version>/share/tentgent/python`
- `/opt/homebrew/Cellar/tentgent/<version>/share/tentgent/scripts`

The direct installer updates:

- `~/.local/bin/tentgent`
- `~/.local/share/tentgent/python`
- `~/.local/share/tentgent/scripts`
- the managed Python runtime under `TENTGENT_HOME/runtime/python-env`

Install and upgrade flows should preserve:

- models
- adapters
- datasets
- train records
- server records
- Keychain secrets
- provider secrets
- other user runtime data under `TENTGENT_HOME`

## Default Layout

Default install locations:

- macOS Homebrew binary: `/opt/homebrew/opt/tentgent/bin/tentgent`
- macOS Homebrew support files: `/opt/homebrew/opt/tentgent/share/tentgent`
- macOS direct-installer binary: `~/.local/bin/tentgent`
- macOS direct-installer support files: `~/.local/share/tentgent`
- macOS runtime home: `~/Library/Application Support/com.tentserv.tentgent`
- Windows binary: `%LOCALAPPDATA%\Programs\tentgent\bin\tentgent.exe`
- Windows support files: `%LOCALAPPDATA%\Programs\tentgent\share\tentgent`
- Windows runtime home: `%LOCALAPPDATA%\tentserv\tentgent\data`
- managed Python runtime: `TENTGENT_HOME/runtime/python-env`
- bootstrap cache: `TENTGENT_HOME/runtime/bootstrap`

Users do not need to preinstall `uv`. Runtime bootstrap downloads pinned bootstrap tools into Tentgent-owned runtime cache.
The managed Python runtime path may differ when `TENTGENT_PYTHON_ENV_DIR` is set.
The bootstrap cache is split by purpose:

- `runtime/bootstrap/uv` contains the pinned installer bootstrap tool cache and should usually be preserved.
- `runtime/bootstrap/uv-cache` contains `uv` package/cache data. It is safe to recreate and may be removed manually when no Tentgent installer or Python bootstrap process is running:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

Do not remove `runtime/python-env` unless you are intentionally repairing or reinstalling the managed Python runtime.

## Runtime Bootstrap

Direct installers run the managed Python base bootstrap by default. Homebrew
installs the CLI and support files only; run the runtime bootstrap explicitly
after install or when Python dependencies change:

```bash
tentgent runtime bootstrap
tentgent doctor
```

The default profile is `base`. Add heavier runtime dependencies only when you
need them:

```bash
tentgent runtime bootstrap --profile local-model
tentgent runtime bootstrap --profile training
tentgent runtime bootstrap --profile full
```

Use `tentgent runtime bootstrap --print-plan` to inspect the resolved project,
environment, cache paths, and selected profile without syncing. `--dry-run`
asks `uv` to plan the sync and may still resolve the pinned bootstrap
tool/cache.

## Uninstall

Remove Homebrew-installed binaries and support files without deleting user
runtime data:

```bash
brew uninstall hiroliang/tap/tentgent
```

Remove direct-installer binaries and support files:

```bash
rm -f "$HOME/.local/bin/tentgent"
rm -rf "$HOME/.local/share/tentgent"
```

```powershell
Remove-Item "$env:LOCALAPPDATA\Programs\tentgent" -Recurse -Force
```

This leaves `TENTGENT_HOME` intact. Keeping runtime home preserves models,
adapters, datasets, sessions, server records, train records, logs, and managed
Python runtime state. To reclaim only safe-to-recreate bootstrap package cache,
remove `runtime/bootstrap/uv-cache` while no Tentgent installer or Python
bootstrap process is running:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

Full runtime-home deletion is destructive. Do it only when you intentionally
want to remove local models, adapters, datasets, sessions, servers, train
records, and other local runtime data. Provider secrets stored in the system
Keychain may need to be removed separately.

## Local Package Smoke Test

From a source checkout, create a release-like artifact:

```bash
scripts/package-local.sh
```

Smoke-test install layout without downloading heavy Python ML dependencies:

```bash
scripts/install.sh \
  --archive dist/tentgent-0.3.3-aarch64-apple-darwin.tar.gz \
  --checksums dist/checksums.txt \
  --prefix /tmp/tentgent-install-smoke \
  --skip-python-bootstrap \
  --skip-doctor
```

Omit `--skip-python-bootstrap` to run the default base managed Python bootstrap,
or run `tentgent runtime bootstrap --profile <profile>` afterward against the
installed support files.
