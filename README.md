# Tentgent

Tentgent is a Rust-first local operator CLI with a Python daemon layer for model runtimes, adapter management, LoRA training, and long-lived local servers.

The current MVP can manage provider keys, pull and deduplicate local models, import or pull LoRA adapters, manage datasets, run one-shot chat, train LoRA adapters, and serve local HTTP chat.

## Languages

- English source of truth: [README.md](./README.md)
- Traditional Chinese: [docs/i18n/zh-TW/README.md](./docs/i18n/zh-TW/README.md)
- Japanese: [docs/i18n/ja/README.md](./docs/i18n/ja/README.md)

## Install

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
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.2/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.2/install.ps1 | iex
```

Then make sure the default install location is on `PATH` and verify the runtime:

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
tentgent doctor
```

Upgrade by running the installer again. User runtime data under `TENTGENT_HOME` is preserved.

See [docs/user/install.md](./docs/user/install.md) for install, upgrade, pinned versions, and local package smoke tests.

## Current Version

`v0.1.2` adds cloud provider server routing and provider-assisted dataset workflows on top of the first installable MVP.

Included:

- provider auth key management for Hugging Face, OpenAI, and Anthropic
- content-addressed model, adapter, and dataset stores
- OpenAI and Anthropic local server proxy runtimes
- dataset validation, prompt templates, provider synthesis, and provider evaluation
- one-shot local chat for MLX, PEFT safetensors, and llama-cpp GGUF paths
- local HTTP chat server with registry and process lifecycle commands
- managed LoRA train plans and runnable MLX / PEFT training loops
- installer-managed Python runtime bootstrap for normal installs

Known limits:

- macOS and Windows x86_64 are the first packaged install targets
- MLX requires Apple Silicon macOS
- HTTP chat streaming is planned but not implemented yet
- macOS signing and notarization are deferred to a later slice

See [docs/user/version.md](./docs/user/version.md) for the version feature list and known limits.

## Quick Start

Pull a small model:

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

Run one-shot chat:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

Run a local server:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

See [docs/user/commands.md](./docs/user/commands.md) for common commands, dataset flow, adapter flow, LoRA training, and server smoke tests.

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

This project is proprietary and all rights are reserved. See [LICENSE](./LICENSE).
