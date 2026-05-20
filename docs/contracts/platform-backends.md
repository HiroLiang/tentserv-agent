# Platform Backends

This document defines Tentgent's platform and backend capability boundary.

## Support Meaning

A model format is not fully supported just because Tentgent can download or store it.

Tentgent treats a backend as supported only when it has:

- a stable routing rule from model format to runtime backend
- a platform capability check before long-running work starts
- dependency diagnostics through `tentgent doctor`
- predictable errors when the backend is unavailable
- documented install expectations

## Current Capability Matrix

Current backend states:

| Backend | Model format | Runtime family | macOS Apple Silicon | macOS Intel | Windows | Linux |
| --- | --- | --- | --- | --- | --- | --- |
| `mlx` | `mlx` | `mlx-lm` | enabled | unsupported | unsupported | unsupported |
| `mlx-vlm` | `mlx` | `mlx-vlm` | planned | unsupported | unsupported | unsupported |
| `mlx-audio` | `mlx` | `mlx-audio` | planned | unsupported | unsupported | unsupported |
| `mlx-diffusion` | `mlx` | `mlx-diffusion` | planned | unsupported | unsupported | unsupported |
| `transformers-peft` | `safetensors` | n/a | dependency-gated | dependency-gated | dependency-gated | dependency-gated |
| `diffusers` | `diffusers` | n/a | dependency-gated | dependency-gated | dependency-gated | dependency-gated |
| `llama-cpp` | `gguf` | n/a | dependency-gated | dependency-gated | dependency-gated | dependency-gated |

State meanings:

- `enabled`: Tentgent may route work to this backend on the current platform.
- `dependency-gated`: Tentgent may route work to this backend, but Python packages or native wheels must still be installed and checked.
- `planned`: Tentgent may record this runtime family in model metadata, but it
  must reject execution until a later slice implements and smoke-tests the
  backend.
- `unsupported`: Tentgent should block before launching the backend.

## Backend Selection

Runtime backend selection follows model capability, model format, and MLX
runtime family:

- `primary_format = "mlx"` with missing family or `mlx_runtime_family =
  "mlx-lm"` uses the existing MLX chat backend for `chat`
- `primary_format = "mlx"` with `mlx-vlm`, `mlx-audio`, or `mlx-diffusion` is
  metadata-only until the matching backend slice is implemented
- `primary_format = "safetensors"` uses `transformers-peft`
- `primary_format = "diffusers"` uses `diffusers`
- `primary_format = "gguf"` uses `llama-cpp`

LoRA training backend selection:

- `mlx` models select `mlx` only on Apple Silicon macOS.
- `safetensors` models select `peft`.
- `gguf` models are blocked for LoRA training.

## Guardrails

Rust should enforce capability checks for:

- `tentgent doctor`
- `tentgent server run`
- `tentgent server start`
- `tentgent train lora plan create`

Python should enforce the same checks before backend creation so `chat` and server requests fail predictably even when invoked directly.

`model pull` may still store a model whose backend is unsupported on the current platform. Stored model metadata should surface backend support so the user understands that the asset exists but cannot run locally.

## Windows Position

Windows x86_64 is packaged in the installable MVP, with PEFT/safetensors as the
first runtime path. MLX remains unsupported on Windows, and `llama-cpp` remains
dependency-gated by native package availability.

The resolver uses the Windows Python environment layout:

```text
<python-env>/Scripts/python.exe
```

Windows release artifacts, PowerShell installation, and installer-owned Python
dependency bootstrap are part of the current packaging path. Backend execution
still depends on the installed Python environment and platform-specific wheels.

## Linux Position

Linux x86_64 is available as a prerelease GitHub Release tarball install path.
The default `base` managed Python runtime profile has been smoke-tested on
Ubuntu 24.04 without build tools and passes `tentgent doctor`.

This does not imply full Linux backend parity. Local-model serving, training,
GPU/CUDA behavior, Linux arm64, Linuxbrew, `.deb`, and `.rpm` distribution are
still separate readiness decisions. Linux backend rows remain
`dependency-gated` until their profile-specific dependencies and smoke tests
are explicit.
