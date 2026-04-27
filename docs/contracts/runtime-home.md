# Runtime Home

This document defines how Tentgent should resolve and use its daemon-managed local storage.

## Purpose

- Keep models, adapters, datasets, training plans, cache, runtime sockets, and logs outside the repository by default.
- Let the CLI and future HTTP entry point share the same persistent local state.
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
- `config.toml`

## Development Usage

- During repository development, run commands from the repository root.
- For isolated local testing, set `TENTGENT_HOME="$PWD/.tentgent"` before starting the CLI or daemon.
- Do not assume the current working directory is the storage root.

## Persistence Rules

- Read environment variables only at process start.
- Do not rewrite or persist environment variables from inside the application.
- Treat environment variables as operator-controlled overrides.
- Treat the platform default runtime home as the fallback when no override is set.
- Reserve `config.toml` as the future persistent config filename, but do not assume config-file loading exists until it is implemented.
