# M6I MLX Vision Chat Backend

Status: implemented and smoke-tested.

Depends on:

- [M6F vision chat image input](./m6f-vision-chat-image-input.md)
- [M6H MLX multimodal backend foundation](./m6h-mlx-multimodal-backend-foundation.md)

## Goal

Make the existing native `vision-chat` workflow run on Apple Silicon through
MLX VLM models. M6I turns the M6H `mlx-vlm` metadata family from a planned
runtime family into a real backend path.

The user-facing contract should stay unchanged:

```bash
tentgent vision chat /path/to/image.png \
  --model-ref <mlx-vlm-model-ref> \
  --prompt "Describe this image." \
  --output answer.md \
  --format md
```

Daemon clients should keep using the existing native endpoint:

```http
POST /v1/vision/chat
Content-Type: multipart/form-data
```

## Product Decisions

- Keep `vision-chat` separate from text-only `chat`.
- Keep the M6F single-image plus prompt request shape.
- Do not add new public API routes. The existing CLI and daemon route should
  choose the backend from model metadata.
- Do not add `tentgent server` vision serving in M6I. Direct media serving is a
  later roadmap decision.
- Do not add OpenAI, Claude, or Gemini multimodal compatibility.
- Do not add multi-image, video, audio, LoRA, adapters, or structured vision
  output in this slice.
- Treat `mlx-vlm` as Apple Silicon only. Other platforms should fail early with
  clear doctor/runtime guidance.

## Runtime Candidate

Primary runtime package:

- `mlx-vlm`

Planning notes as of 2026-05-20:

- PyPI lists `mlx-vlm` as a beta package for MLX VLM and omni-model inference
  on Mac.
- The package exposes a direct Python path using `load`, `generate`,
  `apply_chat_template`, and `load_config`.
- Hugging Face MLX model cards show `mlx_vlm.generate` examples that accept a
  model id or local path plus `--image <path_to_image>`.

Primary smoke model:

```bash
tentgent model pull mlx-community/SmolVLM-256M-Instruct-bf16 \
  --capability vision-chat
```

Why this model:

- MLX VLM model under `mlx-community/*`, so Tentgent detects
  `primary_format = "mlx"`.
- `--capability vision-chat` should persist `mlx_runtime_family = "mlx-vlm"`.
- Small enough for Apple Silicon smoke testing, about 518 MiB on Hugging Face.
- Apache-2.0 license.

Fallback candidates:

- `mlx-community/LFM2-VL-450M-4bit`, smaller but license/custom-code behavior
  needs explicit verification.
- `mlx-community/Qwen2.5-VL-3B-Instruct-4bit`, larger at about 3.07 GiB but
  useful as a stronger follow-up smoke target.

## Execution Plan

### 1. Runtime Dependency And Bootstrap

- Added `mlx-vlm` to the Python `local-model` optional dependency for Apple
  Silicon macOS only:

```toml
"mlx-vlm; sys_platform == 'darwin' and platform_machine == 'arm64'"
```

- Ran `uv lock`.
- Confirmed the updated `local-model` dependency set can import `mlx_vlm` on
  Apple Silicon with `uv run --extra local-model python -c "import mlx_vlm"`.
- Keep `ruff` out of runtime dependencies; it remains dev-only/temporary.

### 2. Doctor And Capability Readiness

- Added a dedicated backend readiness kind for MLX VLM instead of overloading
  existing `BackendKind::Mlx`.
- Probe Python modules:

```text
mlx
mlx_vlm
```

- Reported the backend as:
  - `ready` on Apple Silicon when imports work
  - `missing` with `tentgent runtime bootstrap --profile local-model` guidance
  - `unsupported` outside Apple Silicon macOS
- Updated doctor backend labels and tests.

### 3. Kernel Vision Backend Routing

- Extended `VisionChatBackend` with an MLX VLM variant:

```rust
VisionChatBackend::MlxVlm
```

- Added a format-plus-family selector:

```rust
VisionChatBackend::from_model_format_and_mlx_family(
    ModelFormat::Mlx,
    Some(MlxRuntimeFamily::Vlm),
) -> Some(VisionChatBackend::MlxVlm)
```

- Kept `ModelFormat::Safetensors` routed to
  `TransformersImageTextToText`.
- Reject MLX models with missing family or non-`mlx-vlm` family for
  `vision-chat`.
- Preserved existing error text that includes the MLX runtime family when a
  model is unsupported.

### 4. Python Router And Backend Factory

- Changed `resolve_vision_chat_backend()` so
  `primary_format = "mlx"` plus `mlx_runtime_family = "mlx-vlm"` returns
  `BackendKind.MLX_VLM`.
- Kept the planned-backend errors for `mlx-audio` and `mlx-diffusion`.
- Updated `ensure_backend_supported()` to treat `mlx_vlm` as Apple Silicon
  only.
- Updated `create_vision_chat_backend()` to instantiate the new backend.

### 5. Python MLX VLM Backend

Added:

```text
python/tentgent-daemon/src/tentgent_daemon/backends/mlx_vlm.py
```

Backend contract:

- Implements `VisionChatBackend`.
- Loads once from `record.variant_source_path`.
- Uses the complete image file path from `VisionChatRequest.image_path`.
- Formats one image plus prompt with the package chat template helper.
- Maps `max_tokens` and `temperature` from existing M6F options.
- Returns `VisionChatResult` with:
  - `output_format`
  - `media_type`
  - generated `text`
  - `finish_reason = "stop"` unless the runtime exposes a stronger signal

Likely package API shape to verify during implementation:

```python
from mlx_vlm import load, generate
from mlx_vlm.prompt_utils import apply_chat_template
from mlx_vlm.utils import load_config
```

The backend should avoid `mlx_vlm.server`; Tentgent owns the CLI/daemon
request contract and only needs the direct Python inference API.

### 6. CLI And Daemon Behavior

- Did not add new CLI flags.
- Did not add new daemon routes.
- Existing flows should work after routing:

```bash
tentgent vision chat test-data/test_image.png \
  --model-ref <mlx-vlm-ref> \
  --prompt "Describe this image." \
  --max-tokens 64
```

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/vision/chat \
  -F model_ref=<mlx-vlm-ref> \
  -F prompt="Describe this image." \
  -F image=@test-data/test_image.png
```

- Confirm `--plan-only` output from `tentgent-vision-chat-once` reports
  `mlx_vlm`.

### 7. Tests

Rust tests:

- `VisionChatBackend` maps `ModelFormat::Mlx + Some(MlxRuntimeFamily::Vlm)` to
  the MLX VLM backend.
- Vision resolver accepts MLX VLM models advertising `vision-chat`.
- Vision resolver rejects MLX chat/audio/diffusion families for `vision-chat`.
- Doctor/capability probe reports `mlx-vlm` readiness separately from `mlx-lm`.

Python tests:

- Router returns `BackendKind.MLX_VLM` for MLX VLM records.
- Router rejects MLX VLM on unsupported platform through the same support gate
  as MLX chat.
- Backend factory returns the MLX VLM backend.
- Backend unit test can monkeypatch package dependencies and assert:
  - local model path is used
  - one image path is passed
  - prompt options map through
  - response text is normalized

Smoke tests:

- Pull `mlx-community/SmolVLM-256M-Instruct-bf16` with
  `--capability vision-chat`.
- Verify `model inspect` shows:
  - `primary_format = mlx`
  - `mlx_runtime_family = mlx-vlm`
  - `model_capabilities = vision-chat`
- Run CLI against a small local image.
- Run daemon multipart endpoint against the same image.
- If package/model loading fails, keep code/tests but record the exact blocker
  in this plan before marking implemented.

Smoke evidence:

- Runtime import passed:

```bash
uv run --extra local-model python -c "import mlx_vlm; print('mlx_vlm import ok')"
```

- Smoke model pull passed:

```bash
cargo run -p tentgent-cli -- model pull \
  mlx-community/SmolVLM-256M-Instruct-bf16 \
  --capability vision-chat
```

Observed metadata:

```text
model_ref = 59f5ddaa302b2ab36b8fe1339c148db9ef7782cd0d1413d79afb9d1f160ee16f
primary_format = mlx
detected_formats = mlx, safetensors
mlx_runtime_family = mlx-vlm
model_capabilities = vision-chat
size = 493.9 MiB
```

- CLI smoke passed:

```bash
cargo run -p tentgent-cli -- vision chat test-data/test_image.png \
  --model-ref 59f5ddaa302b \
  --prompt "Describe this image in one short sentence." \
  --max-tokens 64 \
  --format text
```

- Daemon multipart smoke passed against an isolated test daemon at
  `127.0.0.1:8791` with a symlinked model store:

```bash
curl -sS -X POST http://127.0.0.1:8791/v1/vision/chat \
  -F model_ref=59f5ddaa302b \
  -F prompt="Describe this image in one short sentence." \
  -F max_tokens=64 \
  -F output_format=text \
  -F image=@test-data/test_image.png
```

The daemon returned the full stored model ref, `output_format = "text"`,
generated text, and `finish_reason = "stop"`.

### 8. Documentation

- Updated this plan with implementation notes and smoke evidence.
- Updated [platform-backends.md](../../contracts/platform-backends.md) to mark
  `mlx-vlm` ready/dependency-gated as appropriate after tests.
- Updated [model-fixtures.md](../../user/model-fixtures.md) with the MLX VLM smoke
  command and current caveats.
- Updated [runtime.md](../../user/runtime.md) because doctor output gains
  a new backend label.

## Acceptance Criteria

- Existing Transformers-based `vision-chat` still works.
- MLX VLM models stored with `mlx_runtime_family = "mlx-vlm"` route to the new
  backend.
- `chat` does not accidentally accept `mlx-vlm` models.
- `vision-chat` does not accidentally accept `mlx-lm`, `mlx-audio`, or
  `mlx-diffusion` models.
- Doctor reports MLX VLM readiness independently enough for users to know what
  to install or why the backend is unsupported.
- CLI and daemon multipart smoke tests pass on Apple Silicon with a real MLX
  VLM model, or the plan records a concrete package/model blocker.
