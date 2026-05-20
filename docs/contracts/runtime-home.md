# Runtime Home

This document defines how Tentgent should resolve and use its daemon-managed local storage.

## Purpose

- Keep models, adapters, datasets, training plans, cache, runtime sockets, and logs outside the repository by default.
- Let the CLI and daemon REST entry point share the same persistent local state.
- Make development testing easy to isolate without changing production defaults.

## Naming

- Product slug: `tentgent`
- Binary name: `tentgent`
- Service host: `agent.tentserv.com`
- App identifier: `com.tentserv.tentgent`
- Environment variable prefix: `TENTGENT_`

## Resolution Order

Resolve paths in this order:

1. Use a specific directory override environment variable if it is set.
2. Otherwise use `TENTGENT_HOME` plus the standard subdirectory name.
3. Otherwise derive the platform default base directory from the fixed project identity:
   `ProjectDirs::from("com", "tentserv", "tentgent")`

Environment variables:

- `TENTGENT_HOME`
- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`
- `TENTGENT_TRAIN_DIR`
- `TENTGENT_CACHE_DIR`
- `TENTGENT_RUNTIME_DIR`
- `TENTGENT_LOG_DIR`
- `TENTGENT_PYTHON_DIR`
- `TENTGENT_PYTHON_ENV_DIR`

## Standard Subdirectories

- `models/`
- `servers/`
- `adapters/`
- `datasets/`
- `train/`
- `cache/`
- `runtime/`
- `logs/`
- `locks/`

Reserved runtime files:

- `runtime/tentgent.sock`
- `runtime/tentgent.pid`
- `runtime/daemon.toml`
- `runtime/auth.toml`
- `runtime/capabilities.toml`
- `runtime/jobs/`
- `runtime/bootstrap/`
- `config.toml`

Daemon status and doctor-style diagnostics may inspect these paths in read-only
mode. Read-only inspection must not create missing runtime directories, remove
metadata, or terminate processes. Cleanup-capable daemon status may remove stale
pid/process metadata only after confirming the recorded pid is not running.
Missing runtime-home states should be reported with stable warning codes such as
`runtime_home_missing`, `runtime_dir_missing`, `process_path_missing`,
`pid_path_stale`, or `process_metadata_stale`.

## Auth Metadata

`runtime/auth.toml` stores non-secret provider auth metadata such as keychain
presence, validation state, and last update/validation timestamps. It must not
contain provider secret values, daemon bearer tokens, or request-provided
provider keys.

## Runtime Jobs And Workspaces

`runtime/jobs/` stores local job records and job-scoped temporary work data
through the kernel job workspace contract. Job metadata is persisted as
`<job_id>.json`. Large binary input and result data may live under
`runtime/jobs/<job_id>/workspace/` as chunk files plus done manifests, as
defined in [job-workspace.md](./job-workspace.md).

Job workspace data is temporary runtime state, not a managed model, adapter, or
dataset store. Cleanup must preserve a retention buffer after completion,
interruption, or shutdown so future retry and result-read behavior has a stable
base.

## Shared Config

`config.toml` stores non-secret local preferences shared by the CLI and daemon.
The current schema is:

```toml
schema_version = 1

[daemon]
url = "http://127.0.0.1:8790"
```

Rules:

- Config loading should tolerate unknown fields for forward compatibility.
- Saves should be atomic: write a temp file, fsync when practical, then rename.
- `daemon.url` must be an absolute `http` or `https` URL.
- Provider secrets and `TENTGENT_DAEMON_TOKEN` must never be written to config.
- Environment variables remain operator-controlled overrides over config.

## Development Usage

- During repository development, run commands from the repository root.
- For isolated local testing, set `TENTGENT_HOME="$PWD/.tentgent"` before starting the CLI or daemon.
- Do not assume the current working directory is the storage root.

## Python Runtime Assets

Rust commands that need Python should resolve the Python daemon project without depending on the current working directory.

Python project resolution order:

1. Use `TENTGENT_PYTHON_DIR` when set.
2. Otherwise look for an installed project relative to the `tentgent` binary:
   `../share/tentgent/python`, then `../libexec/tentgent/python`.
3. Otherwise fall back to the repository development project at `python/tentgent-daemon`.

The resolved Python project directory must contain `pyproject.toml`.

Python environment resolution order:

1. Use `TENTGENT_PYTHON_ENV_DIR` when set.
2. For installed-prefix mode, use `TENTGENT_HOME/runtime/python-env`.
3. For development-source or explicit Python-project override mode, use `<python-project>/.venv`.

Rust should pass the resolved Python environment to `uv` as `UV_PROJECT_ENVIRONMENT` when invoking Python helpers through `uv run`.
That fallback is allowed only for development-source or explicit override mode.
Installed-prefix runtime commands must use generated entry points from the managed Python environment and must not fall back to a user PATH `uv`.

`tentgent doctor --fix` should use the same resolved Python project and environment, then run `uv --no-config sync --project <resolved-python-project>` with `UV_PROJECT_ENVIRONMENT=<resolved-python-env>`.
This remains a developer bootstrap path. Public installers must not require users to preinstall `uv`; direct installers own bootstrap automatically, while package-manager installs should expose `tentgent runtime bootstrap` as the user-facing managed-runtime setup entry point.

Installed release artifacts should place Python project files at:

```text
share/tentgent/python/
```

That directory is the packaged equivalent of the repository `python/tentgent-daemon/` project root.

## Bootstrap Cache

`runtime/bootstrap/` is reserved for installer-owned bootstrap tools such as a pinned `uv` executable.
It is not part of the normal runtime command path after the managed Python environment has been created.

Default path:

```text
TENTGENT_HOME/runtime/bootstrap/
```

Rules:

- The public installer may create this directory when it needs to download bootstrap tools.
- Normal runtime commands should not require this directory to exist.
- `tentgent doctor` should report whether the directory exists or can be created, but missing cache state should not block runtime health by itself.
- Downloaded tools should live under versioned, platform-specific child directories.
- Each downloaded tool should have a checksum or manifest record before it is used.

Pinned `uv` bootstrap layout:

```text
runtime/bootstrap/
└── uv/
    └── <version>/
        └── <target>/
            ├── bin/
            │   └── uv
            ├── manifest.toml
            └── sha256.sum
```

`scripts/bootstrap-uv.sh` is the current installer-facing helper for this layout.
It downloads a pinned `uv` release archive, verifies the pinned `sha256.sum` manifest first, verifies the selected archive from that manifest, then writes the executable and `manifest.toml` into the cache.

`tentgent runtime bootstrap` is the public CLI entry point for managed Python environment creation in package-manager installs. It resolves the packaged Python project and managed environment, then delegates to `scripts/bootstrap-python-env.sh` so the pinned `uv` bootstrap behavior stays centralized.

`scripts/bootstrap-python-env.sh` is installer-facing plumbing.
It resolves the packaged or development Python project, ensures pinned `uv` is cached, and runs:

```text
UV_PROJECT_ENVIRONMENT=<python-env> UV_CACHE_DIR=<bootstrap-uv-cache> uv --no-config sync --project <python-project> --managed-python --python 3.13 --frozen --no-editable --reinstall-package tentgent-daemon
```

Default managed Python environment:

```text
TENTGENT_HOME/runtime/python-env/
```

After this step, normal runtime commands should use the generated entry points under `runtime/python-env/bin/` and should not invoke `uv`.
The bootstrap helper also keeps `uv` package/cache data under `runtime/bootstrap/uv-cache/` unless `TENTGENT_BOOTSTRAP_UV_CACHE_DIR` is explicitly set.
`runtime/bootstrap/uv-cache/` is safe-to-recreate cache data and may be removed manually when no installer/bootstrap process is running. `runtime/python-env/` is managed runtime state and should not be removed unless intentionally repairing or reinstalling the Python runtime.

## Persistence Rules

- Read environment variables only at process start.
- Do not rewrite or persist environment variables from inside the application.
- Treat environment variables as operator-controlled overrides.
- Treat the platform default runtime home as the fallback when no override is set.
- Use `config.toml` only for non-secret preferences. Secret material belongs in
  environment variables or the system Keychain according to its contract.
