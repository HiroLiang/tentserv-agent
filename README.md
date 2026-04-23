# Tentgent

Tentgent is a Rust-first local operator CLI with a persistent Python daemon layer for model backends, adapter management, and runtime selection.

The current MVP includes provider-auth key management plus a managed local model store with content-based deduplication.

Language:
- English (source of truth): [README.md](./README.md)
- Traditional Chinese: [docs/i18n/zh-TW/README.md](./docs/i18n/zh-TW/README.md)
- Japanese: [docs/i18n/ja/README.md](./docs/i18n/ja/README.md)

## Install Status

Tentgent is currently source-first.

- Available today:
  - build and run from this repository
- Planned later:
  - Homebrew install
  - packaged app or daemon distribution
  - simpler bootstrap commands for non-developer users

Until packaged installers exist, the supported path is to run Tentgent from the checked-out repository.

## Quick Start

- Build the Rust workspace:

```bash
cargo build --workspace
```

- Pull a small model into the repository-local runtime home:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

- Run one-shot chat:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent chat c5066419e049 --message "user:Hello there"
```

- Launch a long-lived server in foreground mode:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent server run c5066419e049 --host 127.0.0.1 --port 8780 --lazy-load
```

- Launch a long-lived server in background mode:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent server run c5066419e049 --host 127.0.0.1 --port 8780 --lazy-load --detach
```

## Repository Layout

- `src/tentgent-core/`
  Shared Rust core types, runtime contracts, and routing logic. Connected as a workspace library crate.
- `src/tentgent-cli/`
  Rust CLI entry crate. The `tentgent` binary lives here.
- `src/tentgent-http/`
  Rust HTTP entry crate.
- `python/tentgent-daemon/`
  Standalone Python subproject for daemon-side runtime work. This subtree owns its own `pyproject.toml`.
- `python/tentgent-daemon/src/tentgent_daemon/`
  Importable Python package for runtime contracts, backend adapters, CLI helpers, and internal tools.
- `docs/contracts/`
  Reserved for interface documents between Rust entry points, shared core logic, and the Python daemon.
- `Makefile`
  Root developer shortcuts for formatting, checking, building, and running the Rust workspace.

## Naming

- Product slug: `tentgent`
- Binary name: `tentgent`
- Service host: `agent.tentserv.com`
- App identifier: `com.tentserv.tentgent`
- Environment variable prefix: `TENTGENT_`

## Documentation Workflow

- Start with [AGENTS.md](./AGENTS.md) for shared repository context and documentation routing.
- Continue with [CLAUDE.md](./CLAUDE.md) for agent workflows, role definitions, and write boundaries.
- Use `docs/contracts/` for cross-language and cross-module interface notes.
- Use `docs/plans/` for active staged execution plans before large runtime or backend changes.
- Use `docs/plans/archive/` only when historical plan context is actually needed.
- Use folder-level `README.md` files as routing documents when a subtree becomes large enough to justify local navigation.
- The root `pyproject.toml` now keeps shared repository metadata plus workspace-level Pyright paths.
- `python/tentgent-daemon/pyproject.toml` owns Python packaging, dependencies, and entry points for the daemon subproject.

## Runtime Home

- The CLI and any future HTTP entry point should use the same daemon-managed runtime home instead of storing runtime state inside the repository by default.
- The default location should be derived from the platform using the fixed app identifier `com.tentserv.tentgent`.
- Override paths with environment variables when needed:
  - `TENTGENT_HOME`
  - `TENTGENT_MODELS_DIR`
  - `TENTGENT_ADAPTERS_DIR`
  - `TENTGENT_CACHE_DIR`
  - `TENTGENT_RUNTIME_DIR`
  - `TENTGENT_LOG_DIR`
- Standard runtime-home subdirectories now include:
  - `models/`
  - `servers/`
  - `adapters/`
  - `cache/`
  - `runtime/`
  - `logs/`

## Development Workflow

- Yes, repository development and manual testing should be run from the repository root.
- During development, keep runtime files isolated from the global user state by setting `TENTGENT_HOME="$PWD/.tentgent"` before running commands.
- For repository-local testing, prefer one stable runtime home: `TENTGENT_HOME="$PWD/.tentgent-test"`.
- Keep only `.tentgent-test/` as the long-lived repository-local test store. Temporary one-off `.tentgent-*` directories should be deleted after the experiment is finished.
- Installed binaries should fall back to the default platform-managed runtime home when these environment variables are not set.
- Environment variables are read when the process starts. They are not rewritten or persisted by the application.
- Persistent overrides should be stored by the operator in shell startup files, service definitions, or launcher configuration rather than being written back into the repository.
- The root Rust workspace now points to `src/tentgent-core`, `src/tentgent-cli`, and `src/tentgent-http`.
- Use `make check` from the repository root to validate the Rust workspace.
- Use `make run-cli` from the repository root for local CLI testing.
- Use `make run-cli ARGS='--help'` to inspect the current CLI MVP.
- Use `make run-cli ARGS='auth hf'`, `make run-cli ARGS='auth hf set'`, `make run-cli ARGS='auth openai'`, or `make run-cli ARGS='auth anthropic'` to test provider auth flows.
- Use `make run-cli ARGS='help auth'`, `make run-cli ARGS='auth --help'`, and `make run-cli ARGS='auth hf set --help'` to inspect layered CLI help.
- Use `make run-cli ARGS='model --help'` to inspect the model-store command group.
- Use `make run-cli ARGS='model add /path/to/model.gguf'` to import a local model file.
- Use `make run-cli ARGS='model pull google/gemma-3-1b-pt'` to pull a full Hugging Face snapshot into the managed store.
- Use `make run-cli ARGS='model rm <hash>'` to remove a managed model and its source indexes. Tentgent blocks removal while any stored server spec still references that model.
- Use `make run-cli ARGS='model ls'` and `make run-cli ARGS='model inspect <short-ref>'` to inspect stored models.
- Use `python/tentgent-daemon/.venv/bin/tentgent-chat-once --model-ref <REF> --message "user:..."` to exercise the Python-first chat harness directly without `uv` workspace warnings.
- Use `PYTHONPATH=python/tentgent-daemon/src python/tentgent-daemon/.venv/bin/python -m tentgent_daemon.cli.server --server-ref <server-ref> --model-ref <model-ref> --host 127.0.0.1 --port 8000` to exercise the Slice 3 Python server skeleton directly.
- Use `./target/debug/tentgent chat <model-ref>` to run the Rust wrapper around the Python chat harness.
- Use `./target/debug/tentgent server run <model-ref>` to persist a server spec and launch the Python server skeleton in foreground mode by default.
- Use `./target/debug/tentgent server run <model-ref> --detach` to create or reuse the server spec and return immediately after launching the Python server in background mode.
- `tentgent server run` now persists a server spec under `TENTGENT_HOME/servers/<server_ref>/server.toml` and forwards control to the Python server entry.
- Use `./target/debug/tentgent server ls --home "$PWD/.tentgent-test"` to list stored server specs.
- Use `./target/debug/tentgent server ps --home "$PWD/.tentgent-test"` to list live server processes.
- Use `./target/debug/tentgent server inspect <server-ref> --home "$PWD/.tentgent-test"` to inspect one server spec and its runtime state.
- Use `./target/debug/tentgent server start <server-ref> --home "$PWD/.tentgent-test"` to relaunch one stored server spec in background mode.
- Add `--details` to `server start` when you want the full inspection table after launch.
- Use `./target/debug/tentgent server stop <server-ref> --home "$PWD/.tentgent-test"` to stop one live server process without deleting its spec.
- Add `--details` to `server stop` when you want the full inspection table after shutdown.
- Use `./target/debug/tentgent server rm <server-ref> --home "$PWD/.tentgent-test"` to remove one stopped server spec directory.
- Add `--details` to `server rm` when you want the full inspection table captured before removal.
- The current server skeleton exposes:
  - `GET /healthz`
  - `POST /v1/chat`
- HTTP `stream=true` is not implemented yet and currently returns `501`.
- Lifecycle policy is now explicit:
  - eager load when `--lazy-load` is absent
  - load on first request when `--lazy-load` is present
  - idle release when `--idle-seconds` is set

## Repository-Local Test Commands

- Build the Rust workspace:

```bash
cargo build --workspace
```

- Inspect CLI help with the repository-local runtime home:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent --help
```

- Pull a small Hugging Face model into `.tentgent-test/`:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

- Pull a small MLX model for Apple Silicon testing:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
```

- Pull a small GGUF model for llama.cpp testing:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

- List stored models:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model ls
```

- Inspect one stored model:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model inspect <short-ref>
```

- Remove one stored model by hash or short hash:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model rm <hash>
```

- Run one-shot Python chat directly against a stored `safetensors` model:

```bash
python/tentgent-daemon/.venv/bin/tentgent-chat-once --model-ref <short-ref> --home "$PWD/.tentgent-test" --message "user:Hello there"
```

- Stream generated text to stdout:

```bash
python/tentgent-daemon/.venv/bin/tentgent-chat-once --model-ref <short-ref> --home "$PWD/.tentgent-test" --message "system:You are terse." --message "user:Hello there" --stream
```

- Message inputs accept `role:content` for ordered context. Supported roles are `system`, `user`, and `assistant`. If no role prefix is present, Tentgent treats the message as `user`.
- A verified small MLX test model is `mlx-community/Llama-3.2-1B-Instruct-4bit`, which Tentgent stores as `primary_format = "mlx"` when pulled through the model store.
- A verified small GGUF test model is `DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF`, which Tentgent stores as `primary_format = "gguf"` when pulled through the model store.

- Run one-shot Rust chat against a stored model:

```bash
./target/debug/tentgent chat <short-ref> --home "$PWD/.tentgent-test" --message "user:Hello there"
```

- The Rust `tentgent chat` wrapper is the preferred end-user path because it avoids `uv` workspace warnings and suppresses backend stderr noise during successful chats.

- Omit `--message` to let the Rust wrapper prompt once for terminal input:

```bash
./target/debug/tentgent chat <short-ref> --home "$PWD/.tentgent-test"
```

- Stream generated text through the Rust wrapper:

```bash
./target/debug/tentgent chat <short-ref> --home "$PWD/.tentgent-test" --message "system:You are terse." --message "user:Hello there" --stream
```

## Keychain Prompts

- On macOS, Tentgent may trigger a Keychain access prompt when a command needs a stored provider secret and no environment-variable override is present.
- This is expected for commands such as `tentgent auth hf`, `tentgent auth openai`, `tentgent auth anthropic`, and `tentgent model pull <HF_REPO>` when the effective secret source is the system keychain.
- `tentgent model ls` and `tentgent model inspect <REF>` should not read provider secrets and should not require Keychain access.
- If you trust your locally built `./target/debug/tentgent` binary on your own machine, choosing `Always Allow` is a reasonable development workflow. If you are unsure about the binary identity or build source, prefer `Allow`.
- Rebuilding or relocating an unsigned development binary may cause macOS to ask again because the system may treat it as a different requester.
- To skip Keychain reads for a single command, provide the provider secret as a one-shot environment variable. For example:

```bash
HF_TOKEN="your token" TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

- One-shot environment variables like the example above apply only to that single command and do not need `unset`.

## Documentation Maintenance

- When an approved change affects structure, contracts, entry points, or operating boundaries, update the affected Markdown files in the same change.
- Keep Markdown concise and split by concern.
- If a subtree grows, expand with folders plus `README.md` instead of letting one file become a large mixed document.

## Language Policy

- `README.md` is maintained in English as the primary project entry document.
- Localized README variants live under `docs/i18n/`.
- Markdown files in the repository should be written in English unless they are intentionally placed under `docs/i18n/`.
- English documents are the source of truth for localized counterparts.

## Current Status

- Rust auth-key flows are implemented for Hugging Face, OpenAI, and Anthropic.
- The model-store MVP is implemented with `model add`, `model pull`, `model ls`, `model rm`, and `model inspect`.
- The Python `tentgent-chat-once` harness can now run stored `safetensors` models through the transformers backend, with optional stdout streaming.
- The Python `tentgent-chat-once` harness can now run stored `mlx` models through the MLX backend, with optional stdout streaming.
- The Python `tentgent-chat-once` harness can now run stored `gguf` models through the llama.cpp backend, with optional stdout streaming.
- The Python `tentgent-server` skeleton can now bind a long-lived process, answer `GET /healthz`, and serve non-streaming `POST /v1/chat`.
- The Python server skeleton now reports lifecycle state through `GET /healthz`, including load mode, idle policy, and release metadata.
- The Rust `tentgent chat <MODEL_REF>` command now wraps the Python chat harness and preserves backend behavior, including terminal prompting and `--stream`.
- The Rust `tentgent server run <MODEL_REF>` command now wraps the Python server skeleton in foreground mode by default and supports `--detach` for the initial background launch.
- The Rust `tentgent server run <MODEL_REF>` command now persists a stable server spec and `SERVER_REF` as Slice 2 of the future long-lived server flow.
- The Rust CLI now includes `tentgent server ls`, `ps`, `inspect`, `start`, `stop`, and `rm` for server registry and process control.
- Canonical model identity is content-derived through `model_ref` hashing, not source-name hashing.
- Naming, runtime-home, and environment-variable conventions are defined in repository TOML metadata.
- Documentation rules require Markdown to be updated together with approved structural changes.
