# Tentgent

Tentgent is a local AI workflow operator: a Rust CLI plus a local HTTP daemon
that manages model runtimes, adapters, datasets, LoRA training, chat servers,
and short-lived working sessions on your machine.

Use it when you want one local tool to:

- pull and deduplicate local models, adapters, and datasets
- run one-shot chat or long-lived local chat servers
- expose local workflows through a loopback HTTP API
- synthesize, validate, evaluate, export, and diff datasets
- create LoRA train plans, launch runs, and inspect run logs or metrics
- keep bounded local chat sessions as short-term working context

Tentgent is local-first. Runtime data lives under `TENTGENT_HOME` by default,
and provider secrets can come from `.env` / environment variables or the system
keychain.

## Languages

- English source of truth: [README.md](./README.md)
- Traditional Chinese: [docs/i18n/zh-TW/README.md](./docs/i18n/zh-TW/README.md)
- Japanese: [docs/i18n/ja/README.md](./docs/i18n/ja/README.md)

## Install The Tool

Recommended macOS install from the latest GitHub Release:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
```

Recommended Windows PowerShell install from the latest GitHub Release:

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

Install a pinned version when you want a reproducible setup:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.0-alpha.2/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.0-alpha.2/install.ps1 | iex
```

Then make sure the default install location is on `PATH` and verify the runtime:

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
tentgent doctor
tentgent --version
```

Upgrade by running the installer again. User runtime data under `TENTGENT_HOME` is preserved.

See [docs/user/install.md](./docs/user/install.md) for install, upgrade, pinned versions, local package smoke tests, and uninstall notes.

## Configure Keys

Check the local runtime and provider key state:

```bash
tentgent doctor
tentgent status
tentgent auth status
```

Configure provider keys through the system keychain:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
```

Or use environment variables / `.env` for the current process:

```bash
cat > .env <<'EOF'
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
EOF
```

See [docs/contracts/auth-secrets.md](./docs/contracts/auth-secrets.md) for provider secret resolution and Keychain boundaries.

## Import, Pull, And Remove Models

Pull, inspect, import, and remove managed models:

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent model ls
tentgent model inspect <model-ref>
tentgent model add /absolute/path/to/model
tentgent model rm <model-ref>
```

See [docs/user/commands.md](./docs/user/commands.md#models-and-chat) for full model, adapter, dataset, and chat command examples.

## One-Shot Chat

Run one local request without starting a server:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

For one-shot chat message format and adapter examples, see [docs/user/commands.md](./docs/user/commands.md#models-and-chat).

## Start, Stop, And Chat With Servers

Run a model-bound local server:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
curl -sS http://127.0.0.1:8780/healthz
```

Run a cloud provider server through the same local server surface:

```bash
tentgent server run openai:gpt-4.1-mini --host 127.0.0.1 --port 8780
tentgent server run claude:claude-sonnet-4-20250514 --host 127.0.0.1 --port 8781
```

Chat with a server directly:

```bash
curl -sS http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

Manage detached servers:

```bash
tentgent server ls
tentgent server ps
tentgent server stop <server-ref>
```

Direct model-server chat is stateless. Use the daemon in the next section for session-aware chat. For server chat request and adapter rules, see [docs/contracts/server-chat.md](./docs/contracts/server-chat.md).

## Start And Stop The Daemon

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

Use daemon chat when you want session-aware routing through a selected server:

```bash
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"server_ref":"<server-ref>","messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

Stop the daemon:

```bash
tentgent daemon stop
```

For the full daemon API, endpoint list, response shapes, auth behavior, and error mapping, see [docs/contracts/http-daemon.md](./docs/contracts/http-daemon.md).

## Enter The TUI

```bash
tentgent tui
```

The TUI is an operator console for daemon discovery, chat, jobs, resources, stores, servers, training, and guarded setup flows.

## Remove The Tool

Remove installed binaries and support files without deleting user runtime data:

```bash
rm -f "$HOME/.local/bin/tentgent"
rm -rf "$HOME/.local/share/tentgent"
```

Optional safe-to-recreate bootstrap cache cleanup:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

Do not remove `TENTGENT_HOME` unless you intentionally want to delete models, adapters, datasets, sessions, servers, train records, and other local runtime data. See [docs/user/install.md](./docs/user/install.md) and [docs/user/runtime.md](./docs/user/runtime.md) for uninstall and runtime-home details.

## Version Notes

- `v0.3.0-alpha.2`: bugfix preview for session context, rolling summaries, daemon/server boundaries, prerelease safety, size display, and runtime footprint visibility.
- `v0.3.0-alpha.1`: TUI preview release with operator console workflows for chat, jobs, resources, store actions, server/training actions, picker-based create flows, session delete, and compact ref display.
- `v0.2.0`: local HTTP daemon parity expansion with store, dataset, server, chat, training, diagnostics, bounded session APIs, and a first TUI setup surface.

See [docs/user/version.md](./docs/user/version.md) for version notes, feature lists, and known limits.

## Full CLI Command Reference

The README intentionally shows the shortest path. See [docs/user/commands.md](./docs/user/commands.md) for the complete CLI command reference covering TUI, auth, models, adapters, datasets, chat, servers, daemon, sessions, and LoRA training.

## API And Contracts

Detailed contracts live under [docs/contracts/](./docs/contracts/README.md) so
this README stays easy to scan.

- [docs/contracts/http-daemon.md](./docs/contracts/http-daemon.md)
  Complete local daemon API contract, endpoint list, auth behavior, response
  shapes, and error mapping.
- [docs/contracts/server-chat.md](./docs/contracts/server-chat.md)
  Model-bound server chat request shape and adapter validation rules.
- [docs/contracts/session-store.md](./docs/contracts/session-store.md)
  Session metadata, message records, mutation rules, and bounded compaction.
- [docs/contracts/runtime-home.md](./docs/contracts/runtime-home.md)
  Runtime-home, store-path, Python runtime, and environment override rules.
- [docs/contracts/auth-secrets.md](./docs/contracts/auth-secrets.md)
  Provider secret resolution, `.env` / env behavior, and Keychain boundaries.
- [docs/contracts/training-lora.md](./docs/contracts/training-lora.md)
  Managed LoRA plan and run boundaries.

## Configure Paths

Set `TENTGENT_HOME` to move all normal runtime state:

```bash
export TENTGENT_HOME="$HOME/.tentgent"
```

Use narrower overrides when only one store or runtime path should move:

```bash
export TENTGENT_MODELS_DIR="/Volumes/models/tentgent"
export TENTGENT_DATASETS_DIR="$HOME/datasets/tentgent"
export TENTGENT_PYTHON_DIR="$PWD/python/tentgent-daemon"
export TENTGENT_PYTHON_ENV_DIR="$PWD/python/tentgent-daemon/.venv"
```

Common provider environment variables:

```bash
export HF_TOKEN="..."
export OPENAI_API_KEY="..."
export ANTHROPIC_API_KEY="..."
```

Tentgent loads `.env` for process-local provider credentials before falling
back to the system keychain. For predictable `.env` behavior, run `tentgent`
from the directory containing the file or export variables in your shell.

See [docs/user/runtime.md](./docs/user/runtime.md) for platform defaults,
runtime-home rules, Python runtime resolution, and Keychain prompt notes.

## Current Capabilities

Included:

- provider auth key management for Hugging Face, OpenAI, and Anthropic
- content-addressed model, adapter, and dataset stores
- OpenAI and Anthropic local server proxy runtimes
- dataset validation, prompt templates, multi-split provider synthesis, and provider evaluation
- one-shot local chat for MLX, PEFT safetensors, and llama-cpp GGUF paths
- local HTTP daemon API for store, dataset, server, chat, training, diagnostics, and bounded session workflows
- terminal UI operator console for daemon discovery, chat, jobs, resources,
  store/server/training actions, session cleanup, and guarded local setup
- managed LoRA train plans, durable run records, metrics/log inspection, and runnable MLX / PEFT training loops
- local sessions with bounded transcript compaction for short-term working context
- installer-managed Python runtime bootstrap for normal installs

Known limits:

- macOS and Windows x86_64 are the first packaged install targets
- MLX requires Apple Silicon macOS
- Cloud provider servers do not support request-time local adapters
- generated dataset splits are not deduplicated against each other yet
- provider key set/remove and `doctor --fix` remain CLI-only
- macOS signing and notarization are deferred to a later slice

## Development

Build from source:

```bash
cargo build --workspace
./target/debug/tentgent doctor
```

Use a repository-local runtime home while testing:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

See [docs/development/README.md](./docs/development/README.md) for developer commands and repository-local tests.

## Contributing

Issues, experiments, integrations, and pull requests are welcome. Good first
areas include documentation, installer smoke tests, platform-specific runtime
notes, dataset examples, and clients that use the local HTTP daemon.

Before larger changes, read [AGENTS.md](./AGENTS.md) and the relevant contract
under [docs/contracts/](./docs/contracts/README.md), then keep changes small
enough to review.

## Project Docs

- [docs/user/](./docs/user/README.md)
  User install, upgrade, version, command, runtime, and Keychain docs.
- [AGENTS.md](./AGENTS.md)
  Shared repository context and documentation routing.
- [CLAUDE.md](./CLAUDE.md)
  Agent workflows and role boundaries.
- [docs/contracts/](./docs/contracts/README.md)
  Cross-language interfaces and stable runtime contracts.
- [docs/plans/](./docs/plans/README.md)
  Active staged plans.

## License

This project is licensed under the Apache License, Version 2.0. See [LICENSE](./LICENSE).
