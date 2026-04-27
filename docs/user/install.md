# Install And Upgrade

Use this document for user-facing install and upgrade flows.

## Latest Install

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

## Pinned Install

Use a fixed version when you want reproducible installation:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.0/install.sh | sh
```

The pinned installer is tied to that release's artifact URL and version.

## Upgrade

Upgrade by running the installer again:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
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

- binary: `~/.local/bin/tentgent`
- support files: `~/.local/share/tentgent`
- macOS runtime home: `~/Library/Application Support/com.tentserv.tentgent`
- managed Python runtime: `TENTGENT_HOME/runtime/python-env`
- bootstrap cache: `TENTGENT_HOME/runtime/bootstrap`

Users do not need to preinstall `uv`. The installer downloads pinned bootstrap tools into Tentgent-owned runtime cache.

## Local Package Smoke Test

From a source checkout, create a release-like artifact:

```bash
scripts/package-local.sh
```

Smoke-test install layout without downloading heavy Python ML dependencies:

```bash
scripts/install.sh \
  --archive dist/tentgent-0.1.0-aarch64-apple-darwin.tar.gz \
  --checksums dist/checksums.txt \
  --prefix /tmp/tentgent-install-smoke \
  --skip-python-bootstrap
```

Omit `--skip-python-bootstrap` to run the full managed Python bootstrap.
