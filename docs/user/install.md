# Install And Upgrade

Use this document for user-facing install and upgrade flows.

## Latest Install On macOS

Install the latest GitHub Release:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
```

Then ensure the default install location is on `PATH`:

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
```

Verify the runtime:

```bash
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

Use a fixed version when you want reproducible installation:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.0/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.0/install.ps1 | iex
```

The pinned installer is tied to that release's artifact URL and version.
`v0.3.0` is the stable 0.3.x baseline; use `v0.2.0` if you want the previous
daemon-parity baseline.

## Upgrade

Upgrade by running the installer again:

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

The installer updates:

- `~/.local/bin/tentgent`
- `~/.local/share/tentgent/python`
- `~/.local/share/tentgent/scripts`
- the managed Python runtime under `TENTGENT_HOME/runtime/python-env`

The installer should preserve:

- models
- adapters
- datasets
- train records
- server records
- Keychain secrets
- other user runtime data under `TENTGENT_HOME`

## Default Layout

Default install locations:

- macOS binary: `~/.local/bin/tentgent`
- macOS support files: `~/.local/share/tentgent`
- macOS runtime home: `~/Library/Application Support/com.tentserv.tentgent`
- Windows binary: `%LOCALAPPDATA%\Programs\tentgent\bin\tentgent.exe`
- Windows support files: `%LOCALAPPDATA%\Programs\tentgent\share\tentgent`
- Windows runtime home: `%LOCALAPPDATA%\tentserv\tentgent\data`
- managed Python runtime: `TENTGENT_HOME/runtime/python-env`
- bootstrap cache: `TENTGENT_HOME/runtime/bootstrap`

Users do not need to preinstall `uv`. The installer downloads pinned bootstrap tools into Tentgent-owned runtime cache.
The managed Python runtime path may differ when `TENTGENT_PYTHON_ENV_DIR` is set.
The bootstrap cache is split by purpose:

- `runtime/bootstrap/uv` contains the pinned installer bootstrap tool cache and should usually be preserved.
- `runtime/bootstrap/uv-cache` contains `uv` package/cache data. It is safe to recreate and may be removed manually when no Tentgent installer or Python bootstrap process is running:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

Do not remove `runtime/python-env` unless you are intentionally repairing or reinstalling the managed Python runtime.

## Uninstall

Remove installed binaries and support files without deleting user runtime data:

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
  --archive dist/tentgent-0.3.0-aarch64-apple-darwin.tar.gz \
  --checksums dist/checksums.txt \
  --prefix /tmp/tentgent-install-smoke \
  --skip-python-bootstrap \
  --skip-doctor
```

Omit `--skip-python-bootstrap` to run the full managed Python bootstrap.
