# Tentgent

Tentgent is a local AI workflow operator: a Rust CLI plus a local HTTP daemon
that manages model runtimes, adapters, datasets, LoRA training, chat servers,
and short-lived working sessions on your machine.

Use it when you want one local tool to:

- pull and deduplicate local models, adapters, and datasets
- run one-shot chat or long-lived local chat servers
- expose local workflows through a loopback HTTP API
- run local media workflows such as audio transcription, speech synthesis,
  image generation/editing, vision chat, and video understanding
- validate, import, export, and diff datasets
- create LoRA train plans, launch runs, and inspect run logs or metrics
- keep bounded local chat sessions as short-term working context

Tentgent is local-first. Runtime data lives under `TENTGENT_HOME` by default,
and provider secrets can come from `.env` / environment variables or the system
keychain.

## Languages And Docs

- English source of truth: [README.md](./README.md)
- Traditional Chinese: [docs/i18n/zh-TW/README.md](./docs/i18n/zh-TW/README.md)
- Japanese: [docs/i18n/ja/README.md](./docs/i18n/ja/README.md)
- Full user guide: [docs/user/README.md](./docs/user/README.md)
- HTTP API reference: [docs/user/api.md](./docs/user/api.md)
- Model fixture and smoke-test guide:
  [docs/user/model-fixtures.md](./docs/user/model-fixtures.md)
- Model support catalog:
  [docs/user/model-support-catalog.md](./docs/user/model-support-catalog.md)
- Developer guide: [docs/development/README.md](./docs/development/README.md)

## Quick Start

The current product surface is the `tentgent` CLI plus the local daemon REST
API. There is no terminal UI command.

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
```

Then configure keys only for the providers you use:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
tentgent auth gemini set
```

Try the smallest local workflow:

```bash
tentgent model pull google/gemma-3-1b-it
tentgent model ls
tentgent chat <model-ref> --message "user:Hello"
```

Start the daemon when you want HTTP access:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
curl -sS http://127.0.0.1:8790/healthz
```

## Install The Tool

Recommended macOS install through the project Homebrew tap:

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
tentgent --version
```

Recommended Windows PowerShell install from the latest GitHub Release:

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

Linux x86_64 install from the latest GitHub Release:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | bash
tentgent doctor
```

The Linux release uses the GitHub Release tarball and the default `base`
runtime bootstrap profile. Full managed runtime and local model backend parity
is still not claimed on Linux.

On Linux preview installs, set and persist `TENTGENT_HOME` before bootstrap if
you want runtime data outside the default direct-installer support directory.

Use GitHub Release installers when you want a pinned or reproducible
script-based setup:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.ps1 | iex
```

If you previously installed with `install.sh`, `~/.local/bin/tentgent` may
shadow the Homebrew binary. Check the Homebrew build directly with:

```bash
/opt/homebrew/opt/tentgent/bin/tentgent -V
```

Upgrade Homebrew installs with `brew upgrade hiroliang/tap/tentgent`. User
runtime data under `TENTGENT_HOME` is preserved.

See [docs/user/install.md](./docs/user/install.md) for install, upgrade, pinned versions, local package smoke tests, and uninstall notes.

## Configure Keys

Check the local runtime and provider key state:

```bash
tentgent doctor
tentgent runtime status
tentgent auth status
```

Configure provider keys through the system keychain:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
tentgent auth gemini set
```

Or use environment variables / `.env` for the current process:

```bash
cat > .env <<'EOF'
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GEMINI_API_KEY=...
EOF
```

See [docs/contracts/auth-secrets.md](./docs/contracts/auth-secrets.md) for provider secret resolution and Keychain boundaries.

## Import, Pull, And Remove Models

Pull, inspect, import, and remove managed models:

```bash
tentgent model catalog --capability chat --publisher Qwen
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent model ls
tentgent model inspect <model-ref-or-prefix>
tentgent model add /absolute/path/to/model
tentgent model rm <model-ref>
```

Use `model catalog` to browse built-in model-family support hints before
pulling a model. See
[docs/user/commands.md](./docs/user/commands.md#models-and-chat) for full
model, adapter, dataset, and chat command examples.
`model ls` keeps support status compact, while `model inspect` shows the full
per-capability proof, hint, backend, and reason details.

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

Local servers are launched through the direct Python model runtime daemon for
`chat`, `embedding`, `rerank`, audio, vision, video, and image endpoint
families. The model is bound at server start, so direct server requests stay
small and do not need a `model` payload. When `--capability` is omitted for a
local model, Tentgent chooses the server endpoint family from the model's stored
capabilities, preferring more specialized media capabilities before `chat`.
When `--port` is omitted, server specs start from port `8780` and the launcher
scans upward at process start until it finds a free loopback port. Explicit
`--port` values are fixed and fail clearly when unavailable. Running server
metadata records the actual bound port for later `server ls`, health checks, and
direct curl calls.
Local model-bound server starts also record latest capability proof metadata so
`tentgent model capability proofs <model-ref>` can show which capability paths
have launched or failed locally.

Cloud provider servers are paused until they are ported to the model runtime
HTTP boundary.

Chat with a server directly:

```bash
curl -sS http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

Manage detached servers:

```bash
tentgent server ls
tentgent server inspect <server-ref>
tentgent server ps
tentgent server stop <server-ref>
```

Direct model-server chat is stateless. Use the daemon in the next section for
model-ref based native and compatibility chat routes. For server chat request
and adapter rules, see [docs/contracts/server-chat.md](./docs/contracts/server-chat.md).
For local model-bound servers, `server ls` shows compact model short refs and
`server inspect` shows the full bound model plus selected-capability support
status. Local chat server specs also show the selected runtime profile, such as
`local-chat-mlx-v1`, when profile metadata is recorded.

## Start And Stop The Daemon

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

Use daemon chat when you want the local daemon to run the same text-only chat
use case through native, OpenAI-compatible, Claude-compatible, or
Gemini-compatible request shapes:

```bash
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","messages":[{"role":"user","content":"Hello"}],"stream":false}'

curl -sS http://127.0.0.1:8790/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"<model-ref>","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

Stop the daemon:

```bash
tentgent daemon stop
```

For the full user-facing daemon API, endpoint list, response shapes, auth
behavior, and error mapping, see [docs/user/api.md](./docs/user/api.md). For
the lower-level daemon transport contract, see
[docs/contracts/http-daemon.md](./docs/contracts/http-daemon.md).

## Media CLI And API Rules

- CLI media commands such as `tentgent transcribe`, `tentgent speak`,
  `tentgent vision chat`, `tentgent video understand`, and `tentgent image
  generate` read local files, text, or prompts directly from the caller's
  machine.
- Daemon file-upload media endpoints receive multipart file bytes; curl
  `@/path/file` syntax is client-side file reading, not a daemon path contract.
- Audio transcription, audio speech, video understanding, and image
  generation/editing daemon routes return jobs. Native daemon vision chat is a
  bounded synchronous request.
- Audio/image multipart upload size is controlled by
  `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`, defaulting to 20 MiB. Video uploads use
  `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`, defaulting to 512 MiB. Oversized uploads
  return HTTP `413` with workflow-specific error codes.

See [docs/user/commands.md](./docs/user/commands.md) for CLI examples,
[docs/user/api.md](./docs/user/api.md) for request shapes, and
[docs/user/model-fixtures.md](./docs/user/model-fixtures.md) for small model
fixtures.

## Remove The Tool

Remove Homebrew-installed binaries and support files without deleting user
runtime data:

```bash
brew uninstall hiroliang/tap/tentgent
```

For direct `install.sh` installs, remove the installed files:

```bash
rm -f "$HOME/.local/bin/tentgent"
rm -rf "$HOME/.local/share/tentgent"
```

On Linux preview installs, `$HOME/.local/share/tentgent` may also be the
default runtime home. Do not remove it unless you intentionally want to delete
runtime data or you used `TENTGENT_HOME` to place runtime data elsewhere.

Optional safe-to-recreate bootstrap cache cleanup:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

Do not remove `TENTGENT_HOME` unless you intentionally want to delete models, adapters, datasets, sessions, servers, train records, and other local runtime data. See [docs/user/install.md](./docs/user/install.md) and [docs/user/runtime.md](./docs/user/runtime.md) for uninstall and runtime-home details.

## Version Notes

- `v0.7.0`: support status release; surfaces model capability support in
  `model ls`, `model inspect`, `server ls`, and `doctor`, adds local proof and
  catalog evidence records, and documents how support moves between unknown,
  supported, verified, failed, and unsupported states.
- `v0.6.0`: compatibility contract release; documents OpenAI, Claude/Anthropic,
  and Gemini-compatible endpoint support, stabilizes unsupported provider API
  errors, and adds compatibility fixtures for current daemon, direct cloud, and
  local model-bound provider-shaped routes.
- `v0.5.2`: patch release for readable runtime diagnostics; `runtime status` now renders wrapped field/value rows, and `runtime bootstrap --profile all` aliases `full`.
- `v0.5.1`: patch release for packaged Python workspace layout; includes the workspace `uv.lock` so managed runtime bootstrap works with `uv --frozen`.
- `v0.5.0`: mature model-runtime server release; adds Rust local server proxying through the shared Python model runtime daemon, cloud provider server runtimes, direct local/cloud OpenAI-compatible endpoints, local model capability mutation and proof commands, and the OpenAI `gpt-image-1` request fix.
- `v0.4.1`: signed macOS and M6 media workflow release; adds Developer ID/notarization workflow wiring, native media jobs, MLX media paths, and CLI plus daemon REST consolidation.
- `v0.3.5-alpha.0`: CLI plus daemon REST consolidation preview; removes the former terminal UI, legacy core, and legacy HTTP crates, and keeps broad diagnostics under `doctor`.
- `v0.3.4-alpha.2`: Linux x86_64 preview release with release tarball install, default base runtime bootstrap, and Docker-smoked `doctor` readiness on Ubuntu 24.04.
- `v0.3.3`: adds Homebrew tap update tooling for repeatable formula URL and checksum updates after stable releases.
- `v0.3.2`: adds `tentgent runtime bootstrap` as the package-manager friendly managed Python runtime setup entry point.
- `v0.3.1`: macOS installer hotfix that ad-hoc signs release binaries and clears quarantine metadata after install.
- `v0.3.0`: stable 0.3.x baseline for session context fixes, daemon/server boundaries, release safety, size display, runtime footprint visibility, and improved transcript rendering.
- `v0.3.0-alpha.2`: bugfix preview for session context, rolling summaries, daemon/server boundaries, prerelease safety, size display, and runtime footprint visibility.
- `v0.3.0-alpha.1`: historical terminal UI preview release. The current tool is CLI plus daemon only.
- `v0.2.0`: local HTTP daemon parity expansion with store, dataset, server, chat, training, diagnostics, and bounded session APIs.

See [docs/user/version.md](./docs/user/version.md) for version notes, feature lists, and known limits.

## Full CLI Command Reference

The README intentionally shows the shortest path. See [docs/user/commands.md](./docs/user/commands.md) for the complete CLI command reference covering auth, models, adapters, datasets, chat, servers, daemon, sessions, and LoRA training.

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
export TENTGENT_PYTHON_DIR="$PWD/python/tentgent-model-runtime"
export TENTGENT_PYTHON_ENV_DIR="$PWD/python/tentgent-model-runtime/.venv"
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
- dataset validation, prompt templates, local import, export, and diff workflows
- one-shot local chat for MLX, PEFT safetensors, and llama-cpp GGUF paths
- one-shot local embedding and rerank commands for compatible safetensors models
- foreground audio transcription, text-to-speech WAV generation, native
  image-plus-text vision chat, and image generation/editing for compatible
  local models
- local HTTP daemon API for store, dataset, server, chat, training, diagnostics, and bounded session workflows
- managed LoRA train plans, durable run records, metrics/log inspection, and runnable MLX / PEFT training loops
- local sessions with bounded transcript compaction for short-term working context
- installer-managed Python runtime bootstrap for direct installs and `tentgent runtime bootstrap` for package-manager installs

Known limits:

- macOS and Windows x86_64 are the first packaged install targets
- MLX requires Apple Silicon macOS
- MLX acceleration is currently implemented for chat, LoRA training, native
  vision chat, audio transcription, and image generation. MLX text-to-speech
  remains planned until a stable local `mlx-audio` TTS path is verified.
- Cloud provider servers and provider-backed dataset synth/eval are paused
  until they are ported to the model runtime HTTP boundary
- generated dataset splits are not deduplicated against each other yet
- provider key set/remove and `doctor --fix` remain CLI-only
- macOS Developer ID signing and notarization are handled by the release
  workflow for current macOS artifacts

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
