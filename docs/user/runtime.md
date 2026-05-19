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

Default Linux runtime home:

```text
$HOME/.local/share/tentgent
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

Use `tentgent runtime status` or `tentgent doctor` to inspect
human-readable runtime information. `tentgent runtime status` is scoped to the
managed Python runtime and can be narrowed with `--profile`; `tentgent doctor`
includes broader platform, backend, installation, and runtime footprint
diagnostics.

The managed install default for the Python environment is:

```text
TENTGENT_HOME/runtime/python-env
```

The actual path shown by `runtime status` or `doctor` may differ when
`TENTGENT_PYTHON_ENV_DIR` is set. Treat this environment as required runtime
state. Do not remove it unless you are intentionally repairing or reinstalling
the managed Python runtime.

Package-manager installs such as Homebrew prepare this environment with:

```bash
tentgent runtime bootstrap
```

The default bootstrap profile is `base`, which installs the lightweight Python
helpers needed by common CLI flows. Install local model serving or training
dependencies only when needed:

```bash
tentgent runtime bootstrap --profile local-model
tentgent runtime bootstrap --profile training
tentgent runtime bootstrap --profile full
```

Use `tentgent runtime bootstrap --print-plan` to inspect resolved runtime paths
and selected profile extras without syncing. Direct release installers run the
base bootstrap automatically unless `--skip-python-bootstrap` is passed.

`tentgent runtime bootstrap` options are independent:

- `--project` overrides the Python daemon project.
- `--env` overrides the managed Python environment.
- `--uv` uses an explicit uv executable.
- `--profile` selects `base`, `local-model`, `training`, or `full`.
- `--dry-run` asks uv to plan without syncing.
- `--print-plan` prints the resolved bootstrap plan without syncing.

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
- `tentgent doctor` reports platform and backend capability state.
- `tentgent doctor` verifies `local-model`, `training`, and `full` profile
  readiness by importing the expected Python modules from the selected managed
  Python environment. A successful `full` bootstrap should make GGUF,
  safetensors/PEFT, MLX on Apple Silicon macOS, and training dependencies
  report ready.
- `safetensors` models run through the `transformers-peft` backend when Python dependencies are installed.
- `embedding` safetensors models run through the same local-model Python
  dependency set with `transformers` feature extraction and mean pooling.
- `rerank` safetensors models run through the local-model Python dependency set
  with `transformers` sequence classification.
- `mlx` models run through the MLX backend only on Apple Silicon macOS.
- `gguf` models run through `llama-cpp-python` when that dependency is installed.
- PEFT LoRA adapters can be selected per request with `adapter_ref`.
- MLX adapters can be selected per request; changing adapters reloads the MLX model for correctness.
- HTTP `/v1/chat` returns non-streaming JSON by default.
- Local base-model and compatible adapter requests can use `stream=true` for Server-Sent Events.
- OpenAI and Anthropic cloud provider runtimes can use the same `stream=true` Server-Sent Events shape.
- Windows x86_64 is packaged, but MLX is blocked on Windows.
- Linux x86_64 is available as a prerelease GitHub Release install path. The
  default base Python runtime has been smoke-tested on Ubuntu 24.04 without
  build tools. Local-model, training, GPU, and distro-package parity remain
  dependency-gated.
- Embedding and rerank backend probes verify the local-model `safetensors` /
  `transformers` / `torch` dependency set.

## Media Runtime Dependencies

Media models have two dependency classes:

- Python model execution dependencies are installed by runtime profiles.
  `local-model` covers `torch`, `transformers`, `tokenizers`,
  `safetensors`, and PEFT support used by local safetensors chat,
  embedding, rerank, and `audio-transcription` models.
- Media file decoding dependencies are system tools on `PATH`. Current
  `audio-transcription` jobs store the uploaded file in a daemon workspace and
  pass that file path to the Transformers ASR pipeline, which expects `ffmpeg`
  for MP3, M4A, AAC, Ogg, WebM, MP4, and most compressed audio/video
  containers. Plain WAV/FLAC inputs may still pass through the same decoder
  path, so install `ffmpeg` before running local media jobs.

`tentgent doctor` reports `media decoder ffmpeg` as a warning when the decoder
is missing and prints the primary install command for the current operating
system. On macOS:

```bash
brew install ffmpeg
```

On Debian or Ubuntu:

```bash
sudo apt install ffmpeg
```

Future `audio-speech`, `vision-chat`, `image-generation`, and video-oriented
routes may add more capability-specific checks. Those checks should stay
warning-level unless the user is actively running a feature that requires them.

## Keychain Prompts

On macOS, Tentgent may trigger a Keychain prompt when a command needs a stored provider secret and no environment override is present.

This is expected for commands such as:

- `tentgent auth hf`
- `tentgent auth openai`
- `tentgent auth anthropic`
- `tentgent auth gemini`
- `tentgent model pull <HF_REPO>`
- `tentgent adapter pull <HF_REPO>`

If you trust your installed or locally built `tentgent` binary, choosing `Always Allow` is reasonable. Rebuilding or relocating an unsigned development binary may cause macOS to ask again.

To skip Keychain reads for one command, pass a one-shot environment variable:

```bash
HF_TOKEN="your token" tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

One-shot environment variables apply only to that command and do not need `unset`.

On platforms where native secret storage is unsupported or unavailable,
Tentgent should not store provider keys in plaintext files. Use environment
variables for repeatable headless flows. Commands that need a provider key may
offer a one-operation prompt, and HTTP provider workflows may accept a
per-request secret, but those values are not persistent auth setup.
