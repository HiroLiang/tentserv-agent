# M6K MLX Image Generation Backend Decision

Status: implemented and smoke-tested.

Depends on:

- [M6G image generation jobs](./m6g-image-generation-jobs.md)
- [M6H MLX multimodal backend foundation](./m6h-mlx-multimodal-backend-foundation.md)
- [M6I MLX vision chat backend](./m6i-mlx-vision-chat-backend.md)
- [M6J MLX audio runtime backend](./m6j-mlx-audio-runtime-backend.md)

## Goal

Decide whether Tentgent can support an Apple Silicon MLX backend for the
existing native `image-generation` workflow before implementing advanced image
generation slices.

M6K should not add a new user-facing API. If an MLX backend is approved, the
existing surfaces remain canonical:

```bash
tentgent image generate \
  --model-ref <mlx-image-generation-model-ref> \
  --prompt "A small watercolor cabin at sunrise" \
  --output /absolute/path/image.png \
  --width 512 \
  --height 512 \
  --steps 20 \
  --seed 7
```

```http
POST /v1/images/generations/job
```

If no practical MLX image-generation runtime is stable enough, M6K should record
the blocker explicitly and keep the implemented M6G Diffusers backend as the
supported text-to-image path.

## Implementation Result

M6K approved MFLUX as the first MLX image-generation runtime and implemented a
Flux-family backend behind the existing `image-generation` CLI and daemon job
surfaces.

Implemented paths:

- Kernel capability readiness includes a dedicated `MlxDiffusion` backend kind.
- Doctor reports `backend mlx-diffusion` separately.
- Kernel image-generation resolver maps `ModelFormat::Mlx` plus
  `MlxRuntimeFamily::Diffusion` to an MLX image backend.
- Python router maps `primary_format = "mlx"` plus
  `mlx_runtime_family = "mlx-diffusion"` to `BackendKind.MLX_DIFFUSION`.
- Python backend factory returns `MfluxImageGenerationBackend` for that backend
  kind.
- The Python `local-model` extra includes `mflux` on Apple Silicon macOS.
- `python/tentgent-daemon/src/tentgent_daemon/backends/mlx_diffusion.py` loads
  MFLUX Flux-family models from `record.variant_source_path`, maps the existing
  text-to-image request options, writes the generated image to the Rust-selected
  output path, and returns the same result metadata shape as Diffusers.

Runtime decision:

- MFLUX passed the API spike: it exposes embeddable Python classes, accepts a
  `model_path`, returns a saveable image object, and can be mapped to the
  existing M6G request fields.
- `mlx-community/Flux-1.lite-8B-MLX-Q4` is the current public MLX smoke
  candidate because Tentgent already detects `mlx-community/*` repositories as
  `primary_format = "mlx"` and `--capability image-generation` records
  `mlx_runtime_family = mlx-diffusion`.
- This candidate is about 7 GiB locally, so full model pull and image
  generation smoke should be treated as a deliberate large-model test.
- `filipstrand/Z-Image-Turbo-mflux-4bit` is smaller at about 5.9 GiB and works
  with MFLUX, but it is not under `mlx-community/*`; the current model-store
  format detector would classify it as safetensors unless a later model-format
  detection slice broadens MLX package recognition.

Smoke result:

- `mlx-community/Flux-1.lite-8B-MLX-Q4` pulled successfully with
  `--capability image-generation`.
- Model inspect/pull output showed:
  - `short_ref = 96fdb6180caa`
  - `primary_format = mlx`
  - `detected_formats = mlx, safetensors`
  - `mlx_runtime_family = mlx-diffusion`
  - `model_capabilities = image-generation`
  - `backend_support = dependency-gated: requires MLX image generation Python packages such as mlx and mflux`
- CLI generated a valid 64x64 PNG:

```bash
cargo run -p tentgent-cli -- image generate \
  --model-ref 96fdb6180caa \
  --prompt "A tiny red square" \
  --output /private/tmp/tentgent-m6k-mflux-smoke.png \
  --width 64 \
  --height 64 \
  --steps 1 \
  --guidance-scale 4.0 \
  --seed 1
```

- Daemon smoke used an isolated home on `127.0.0.1:8793`, created
  `job-1779300278124785000-0`, reached `succeeded`, listed
  `m6k-blue-circle.png`, downloaded it, and verified it as a valid 64x64 PNG.
- The isolated daemon was stopped after the smoke test.

Validation completed:

```bash
uv run --extra local-model python -c "import mflux; print('mflux import ok')"
cargo check --workspace
cargo test --workspace
uv run python -m unittest tests.test_image_generation tests.test_runtime_router
uv run --with ruff ruff check src tests
```

## Product Decisions

- Keep `image-generation` separate from `vision-chat` and text-only `chat`.
- Keep M6G text-to-image request and artifact-result contracts unchanged.
- Do not add OpenAI-compatible image APIs.
- Do not add `tentgent server` image-generation routes.
- Do not add image LoRA, image-to-image, inpainting, masks, reference images,
  ControlNet, or upscaling in M6K. Those remain M6L through M6O.
- Do not route `mlx-diffusion` through `mlx-lm`.
- Do not let the Python backend download a different model behind Tentgent's
  model store. The backend should prefer `record.variant_source_path`; any
  runtime that can only run by model id and unmanaged download is a blocker for
  this slice.
- If a runtime requires gated models, record the access requirement and prefer
  a public or clearly documented smoke fixture.

## Runtime Candidates

### Primary Candidate: MFLUX

MFLUX is the first runtime to evaluate because it is an active Python project
for native MLX generative image models. Its README describes it as a native MLX
implementation of generative image models, provides a Python API, supports local
model loading, and includes image-generation families such as Z-Image, FLUX,
FIBO, Qwen Image, and related editing/upscaling features.

Planning sources:

- [`filipstrand/mflux`](https://github.com/filipstrand/mflux)
- [`mlx-community/Flux-1.lite-8B-MLX-Q4`](https://huggingface.co/mlx-community/Flux-1.lite-8B-MLX-Q4)

Open questions for M6K:

- Can a pulled Tentgent model path be passed directly to a stable MFLUX Python
  API without unmanaged re-download?
- Which model family has the smallest practical smoke fixture on Apple Silicon?
- Can text-to-image be normalized to M6G's current options: prompt,
  negative-prompt when supported, width, height, steps, guidance scale, seed,
  and output format?
- Does the package expose stable imports that are appropriate for embedding in
  Tentgent's Python runtime, instead of shelling out to MFLUX CLI commands?
- Does it support `png` output through a PIL-like image object or a stable file
  save path?

### Fallback Candidate: MLX Examples Stable Diffusion

Apple's `mlx-examples/stable_diffusion` includes a Stable Diffusion
implementation in MLX and supports text-to-image. It is useful as a reference
and possibly a smoke path, but it is an example directory rather than a packaged
runtime API.

Planning source:

- [`ml-explore/mlx-examples/stable_diffusion`](https://github.com/ml-explore/mlx-examples/tree/main/stable_diffusion)

Open questions:

- Is it acceptable to vendor or depend on example code?
- Can it load a local model snapshot from Tentgent's store, or does it assume
  direct Hugging Face downloads?
- Are supported model families too narrow for Tentgent's image-generation
  roadmap?

### Not Preferred For M6K: DiffusionKit

DiffusionKit supports Apple Silicon image generation with Core ML and MLX and
has a Python API for MLX image generation. However, its GitHub repository is
archived as of 2026-03-21, so it should not be the first choice for a new
runtime dependency unless the other candidates fail and the archived state is
accepted deliberately.

Planning source:

- [`argmaxinc/DiffusionKit`](https://github.com/argmaxinc/DiffusionKit)

## Decision Checkpoint

M6K has a required checkpoint before code implementation.

Approve MLX implementation only if all of these are true:

- A runtime package imports cleanly through Tentgent's Python environment on
  Apple Silicon.
- A stored local model path can be used as the model source.
- A small or at least practical smoke model can generate one image locally.
- The runtime can save `png`; `jpg` support may be optional if it can be mapped
  cleanly.
- Unsupported options can be ignored or rejected explicitly without changing
  the existing M6G request contract.
- Failures can be surfaced as clear runtime errors and doctor hints.

Record a no-go decision if any of these are true:

- The runtime only works by unmanaged model-id downloads.
- The only practical models are gated or too large for normal Apple Silicon
  smoke testing.
- The package API is CLI-only or too unstable to embed.
- The runtime cannot produce a normal image file through the existing artifact
  contract.
- The dependency set is too invasive for the `local-model` profile.

## Execution Plan

### 1. Runtime Probe Spike

- Install or import the candidate runtime in the Python local-model environment
  without changing user-facing commands.
- Try one local stored model path and one minimal text-to-image request.
- Prefer a low-resolution smoke request:

```text
prompt = "A tiny red square"
width = 64 or the runtime minimum
height = 64 or the runtime minimum
steps = minimum practical value
seed = 1
```

- Record whether the runtime can use `record.variant_source_path`.
- Record whether the output is a PIL image, file path, numpy array, or another
  object that can be normalized.

### 2. Dependency And Capability Readiness

If the runtime is approved:

- Add the runtime package to the Python `local-model` optional dependency for
  Apple Silicon macOS only.
- Run `uv lock`.
- Add a dedicated kernel backend readiness kind, for example:

```text
MlxDiffusion
```

- Probe the selected package modules, for example `mlx` plus `mflux` if MFLUX
  is selected.
- Mark the backend unsupported outside Apple Silicon macOS.
- Update doctor labels and backend support summaries so users see a separate
  `backend mlx-diffusion` status.

If the runtime is rejected:

- Do not add the dependency.
- Keep `mlx-diffusion` as metadata-only.
- Update docs with the blocker and continue M6L through M6O against Diffusers
  first.

### 3. Kernel Image Backend Routing

If approved:

- Extend `ImageGenerationBackend` with an MLX variant:

```rust
ImageGenerationBackend::MlxDiffusion
```

- Add a format-plus-family selector:

```rust
ImageGenerationBackend::from_model_format_and_mlx_family(
    ModelFormat::Mlx,
    Some(MlxRuntimeFamily::Diffusion),
) -> Some(ImageGenerationBackend::MlxDiffusion)
```

- Keep `ModelFormat::Diffusers` routed to the current Diffusers backend.
- Reject MLX models with missing family or non-`mlx-diffusion` family for
  `image-generation`.
- Preserve error text that includes the MLX runtime family.

### 4. Python Router And Backend Factory

If approved:

- Add a Python backend kind for the selected runtime, such as
  `BackendKind.MLX_DIFFUSION`.
- Change `resolve_image_generation_backend()` so `primary_format = "mlx"` plus
  `mlx_runtime_family = "mlx-diffusion"` returns the new backend after
  dependency support checks.
- Update `create_image_generation_backend()` to instantiate the new backend.
- Keep `diffusers` behavior unchanged.

### 5. Python MLX Image Backend

If approved, add:

```text
python/tentgent-daemon/src/tentgent_daemon/backends/mlx_diffusion.py
```

Backend contract:

- Implements `ImageGenerationBackend`.
- Loads from `record.variant_source_path`.
- Accepts the existing `ImageGenerationRequest`.
- Maps common options:
  - prompt
  - width and height
  - steps
  - seed
  - guidance scale when supported
  - negative prompt when supported
- Rejects or warns clearly for unsupported options.
- Writes the generated image to the output path selected by Rust.
- Returns the same `ImageGenerationResult` metadata shape as Diffusers.

Do not call an external runtime server. Tentgent owns daemon lifecycle, job
state, artifact paths, and HTTP shape.

### 6. CLI And Daemon Behavior

No new command or route should be added.

Existing flows should work after routing:

```bash
tentgent image generate \
  --model-ref <mlx-diffusion-ref> \
  --prompt "A tiny red square" \
  --output /private/tmp/tentgent-m6k-mlx-image.png \
  --width 64 \
  --height 64 \
  --steps 2 \
  --seed 1
```

```bash
curl -sS http://127.0.0.1:8790/v1/images/generations/job \
  -H 'content-type: application/json' \
  -d '{
    "model_ref":"<mlx-diffusion-ref>",
    "prompt":"A tiny red square",
    "output_format":"png",
    "width":64,
    "height":64,
    "steps":2,
    "seed":1
  }'
```

Generated files still come from:

```http
GET /v1/images/generations/job/{job_id}/files
GET /v1/images/generations/job/{job_id}/files/{file_id}
```

### 7. Tests

If approved, add Rust tests for:

- `ImageGenerationBackend` maps `ModelFormat::Mlx +
  Some(MlxRuntimeFamily::Diffusion)` to the MLX backend.
- Image-generation resolver accepts MLX diffusion models advertising
  `image-generation`.
- Image-generation resolver rejects MLX chat, VLM, and audio families.
- Capability probe and doctor report `mlx-diffusion` separately.

Add Python tests for:

- Router returns the MLX image backend for `primary_format = "mlx"` plus
  `mlx_runtime_family = "mlx-diffusion"`.
- Unsupported platforms fail through the backend support gate.
- Backend maps the existing request fields into the selected runtime API.
- Backend writes output and returns the same result metadata as the Diffusers
  backend.
- Unsupported options produce clear errors or warnings.

### 8. Smoke Tests

If approved, run both CLI and daemon smoke tests:

```bash
tentgent model pull <approved-mlx-image-model> --capability image-generation
```

```bash
tentgent image generate \
  --model-ref <short-ref> \
  --prompt "A tiny red square" \
  --output /private/tmp/tentgent-m6k-mlx-image.png \
  --width 64 \
  --height 64 \
  --steps 2 \
  --seed 1
```

```bash
file /private/tmp/tentgent-m6k-mlx-image.png
```

Then start a daemon, create an image-generation job with the same model, inspect
the job until it succeeds, list files, download the generated file, and stop the
daemon.

## Documentation Updates

Update:

- `docs/contracts/platform-backends.md`
- `docs/contracts/http-daemon.md`
- `docs/user/commands.md`
- `docs/user/api.md`
- `docs/user/model-fixtures.md`
- `docs/user/runtime.md`
- `docs/user/version.md`
- `docs/plans/README.md`
- `docs/plans/capability-first-release-roadmap.md`

Docs should say either:

- MLX image generation is implemented through the approved runtime, with smoke
  model commands and dependency guidance.

or:

- MLX image generation was evaluated and remains metadata-only, with a concrete
  blocker and a note that Diffusers remains the supported M6 image backend.

## Acceptance Criteria

M6K is complete when one of these outcomes is recorded:

- Implemented path: an MLX image-generation model runs through the existing CLI
  and daemon job API, writes a valid image file, and has tests and docs.
- No-go path: a specific blocker is documented, `mlx-diffusion` remains
  metadata-only, Diffusers remains the supported image-generation backend, and
  the roadmap explicitly allows M6L through M6O to continue on Diffusers first.

In both outcomes, the user-facing M6G API remains stable.
