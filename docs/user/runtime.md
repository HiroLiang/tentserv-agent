# Runtime And Platform Notes

Tentgent stores runtime state outside source code by default.

## Runtime Home

During development, prefer a repository-local runtime home:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Default macOS runtime home:

```text
~/Library/Application Support/com.tentserv.tentgent
```

Default Windows runtime home:

```text
%LOCALAPPDATA%\tentserv\tentgent\data
```

Runtime directories include:

- `models/`
- `adapters/`
- `datasets/`
- `train/`
- `servers/`
- `cache/`
- `runtime/`
- `logs/`

Supported path overrides:

- `TENTGENT_HOME`
- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`
- `TENTGENT_TRAIN_DIR`
- `TENTGENT_CACHE_DIR`
- `TENTGENT_RUNTIME_DIR`
- `TENTGENT_LOG_DIR`

Environment variables are read when a process starts. Tentgent does not rewrite or persist shell environment settings.

## Runtime Footprint

Use `tentgent status` or `tentgent doctor` to inspect human-readable size information for the runtime home, managed Python environment, and bootstrap caches.

The managed install default for the Python environment is:

```text
TENTGENT_HOME/runtime/python-env
```

The actual path shown by `status` or `doctor` may differ when `TENTGENT_PYTHON_ENV_DIR` is set. Treat this environment as required runtime state. Do not remove it unless you are intentionally repairing or reinstalling the managed Python runtime.

Package-manager installs such as Homebrew prepare this environment with:

```bash
tentgent runtime bootstrap
```

Use `tentgent runtime bootstrap --print-plan` to inspect resolved runtime paths
without syncing. Direct release installers run the bootstrap automatically unless
`--skip-python-bootstrap` is passed.

Bootstrap data lives under:

```text
TENTGENT_HOME/runtime/bootstrap
```

Within that directory, `uv/` stores pinned installer bootstrap tooling and should usually be preserved. `uv-cache/` stores package/cache data used while creating or syncing the Python environment; it is safe to recreate. To reclaim that cache manually, only when no Tentgent installer or Python bootstrap process is running:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

## Backend Status

- `tentgent doctor` runs installation and runtime health checks.
- `tentgent status` reports the current platform and backend capability state.
- `safetensors` models run through the `transformers-peft` backend when Python dependencies are installed.
- `mlx` models run through the MLX backend only on Apple Silicon macOS.
- `gguf` models run through `llama-cpp-python` when that dependency is installed.
- PEFT LoRA adapters can be selected per request with `adapter_ref`.
- MLX adapters can be selected per request; changing adapters reloads the MLX model for correctness.
- HTTP `/v1/chat` returns non-streaming JSON by default.
- Local base-model and compatible adapter requests can use `stream=true` for Server-Sent Events.
- OpenAI and Anthropic cloud provider runtimes can use the same `stream=true` Server-Sent Events shape.
- Windows x86_64 is packaged, but MLX is blocked on Windows.

## Keychain Prompts

On macOS, Tentgent may trigger a Keychain prompt when a command needs a stored provider secret and no environment override is present.

This is expected for commands such as:

- `tentgent auth hf`
- `tentgent auth openai`
- `tentgent auth anthropic`
- `tentgent model pull <HF_REPO>`
- `tentgent adapter pull <HF_REPO>`

If you trust your installed or locally built `tentgent` binary, choosing `Always Allow` is reasonable. Rebuilding or relocating an unsigned development binary may cause macOS to ask again.

To skip Keychain reads for one command, pass a one-shot environment variable:

```bash
HF_TOKEN="your token" tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

One-shot environment variables apply only to that command and do not need `unset`.
