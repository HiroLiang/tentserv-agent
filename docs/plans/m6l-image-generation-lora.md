# M6L Image Generation LoRA

Status: implemented and unit-tested; public LoRA fixture smoke pending.

M6L adds one optional image-generation LoRA adapter to the existing native
text-to-image workflow. It extends the M6G and M6K surfaces instead of adding a
new API family.

Implementation summary:

- Added image-generation adapter metadata for target capability, image LoRA
  formats, backend support, selected weight file, trigger words, and scale
  hints.
- Generalized adapter compatibility so callers specify the required model
  capability instead of assuming chat.
- Exposed the same image LoRA metadata on daemon adapter import and pull routes,
  including the detached `/jobs` variants.
- Added one optional adapter selection and LoRA scale to the native image
  generation CLI and daemon job route.
- Wired managed local adapter weight paths through the Python once runtime.
- Implemented Diffusers image LoRA loading and MFLUX `lora_paths` /
  `lora_scales` mapping.
- Covered the new paths with Rust and Python focused tests.

Remaining smoke note:

- No small project-verified public image LoRA fixture is pinned yet. The
  Diffusers and MFLUX paths are implemented and unit-tested with fake runtime
  objects; a future smoke pass should pin one compatible public adapter and
  record the exact commands in user fixtures.

Depends on:

- [M6G image generation jobs](./m6g-image-generation-jobs.md)
- [M6K MLX image generation backend decision](./m6k-mlx-image-generation-backend-decision.md)
- [Adapter store contract](../contracts/adapter-store.md)

## Goal

Allow users to generate an image with one managed LoRA adapter:

```bash
tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --adapter-ref <adapter-ref> \
  --lora-scale 0.8 \
  --prompt "portrait avatar, <trigger words when needed>" \
  --output avatar.png
```

The daemon path should use the same native job endpoint as M6G:

```http
POST /v1/images/generations/job
Content-Type: application/json
```

M6L is adapter selection for text-to-image only. It should prove adapter import,
compatibility validation, runtime loading, CLI execution, daemon job execution,
and documentation before later image-to-image or control workflows build on the
same adapter base.

## Current Context

- M6G already owns the native text-to-image CLI and daemon job contract.
- M6K added the Apple Silicon `mlx-diffusion` backend through MFLUX behind the
  same image-generation request shape.
- The current adapter store recognizes chat-oriented PEFT and MLX LM adapter
  shapes. It also hardcodes chat as the required adapter base capability during
  compatibility checks.
- Image LoRA repositories often contain one or more `.safetensors` files and may
  not include `adapter_config.json`.
- The local MFLUX API accepts `lora_paths` and `lora_scales` at model
  construction time. Tentgent must pass local managed adapter files, not let
  MFLUX download a second copy from Hugging Face.
- The local Diffusers package exposes LoRA support on concrete loaded pipeline
  instances rather than on the base `DiffusionPipeline` class, so runtime code
  must check the loaded pipeline methods and fail clearly when a pipeline does
  not support LoRA.

## Product Decisions

- Do not create a new image LoRA endpoint.
- Extend `tentgent image generate` and `POST /v1/images/generations/job`.
- Keep foreground CLI direct-to-kernel. CLI does not require the daemon.
- Keep daemon image generation as a job that returns existing result file routes.
- Support one LoRA adapter per request in this slice.
- Add `lora_scale` as an optional number. Default to `1.0`; validate it with a
  conservative range such as `0.0..=4.0`.
- Do not auto-inject trigger words into prompts. Store and display trigger-word
  hints when known, but users keep control of the final prompt text.
- Reject incompatible adapters before runtime execution when compatibility cannot
  be proven.
- Do not reuse chat LoRA assumptions for image-generation models.
- Do not add multi-LoRA stacking in M6L.
- Do not train image LoRA in M6L.
- Do not add image-to-image, inpainting, masks, reference images, ControlNet, or
  upscaling in M6L.
- Do not add OpenAI-compatible image APIs or direct `tentgent server` image
  routes in M6L.
- Do not add default model locks for read-only image inference. Operators can
  still control practical concurrency through later runtime capacity settings.

## User Surface

### Adapter Import

M6L should make image LoRA adapters first-class managed adapters. The command
surface should remain the existing adapter group:

```bash
tentgent adapter add <PATH> \
  --base-model-ref <MODEL_REF> \
  --target-capability image-generation \
  --adapter-format diffusers-lora \
  --backend-support diffusers \
  --weight-file pytorch_lora_weights.safetensors \
  --trigger-word "<optional trigger>"
```

```bash
tentgent adapter pull <HF_REPO> \
  --base-model-ref <MODEL_REF> \
  --target-capability image-generation \
  --adapter-format diffusers-lora \
  --backend-support diffusers \
  --weight-file pytorch_lora_weights.safetensors \
  --trigger-word "<optional trigger>"
```

For MFLUX-backed MLX diffusion models:

```bash
tentgent adapter pull <HF_REPO> \
  --base-model-ref <MLX_DIFFUSION_MODEL_REF> \
  --target-capability image-generation \
  --adapter-format mlx-diffusion-lora \
  --backend-support mlx-diffusion \
  --weight-file adapter.safetensors
```

Implementation should infer these fields when the source and base model make the
answer unambiguous. Explicit flags are still needed for ambiguous single-file
`.safetensors` sources.

The daemon adapter import and pull routes should accept the same metadata as JSON
fields, including the detached `/v1/adapters/import/jobs` and
`/v1/adapters/pull/jobs` variants:

```json
{
  "repo_id": "owner/image-lora",
  "base_model_ref": "<image-generation-model-ref>",
  "target_capability": "image-generation",
  "adapter_format": "diffusers-lora",
  "backend_support": ["diffusers"],
  "weight_file": "pytorch_lora_weights.safetensors",
  "trigger_words": ["optional trigger"],
  "recommended_scale": 0.8
}
```

### CLI Generation

Add optional generation flags:

```bash
tentgent image generate \
  --model-ref <MODEL_REF> \
  --adapter-ref <ADAPTER_REF> \
  --lora-scale 0.8 \
  --prompt "..." \
  --output image.png
```

Rules:

- `--adapter-ref` accepts a full adapter ref or unique short-ref prefix.
- `--lora-scale` is valid only when `--adapter-ref` is present.
- Existing output-file protection remains unchanged.
- On success, keep the current short completion line.
- On failure, include whether the adapter was missing, ambiguous, incompatible,
  unsupported by the backend, or rejected by the runtime.

### Daemon Job

Extend the current request body with optional adapter fields:

```json
{
  "model_ref": "<image-generation-model-ref>",
  "adapter_ref": "<adapter-ref>",
  "lora_scale": 0.8,
  "prompt": "A clean product render",
  "output_format": "png",
  "width": 512,
  "height": 512,
  "steps": 20,
  "seed": 7
}
```

Rules:

- The route remains `POST /v1/images/generations/job`.
- Result list and file routes remain unchanged.
- `GET /v1/jobs/{job_id}` should expose enough target/artifact metadata to see
  which base model and adapter were requested.
- Pending, failed, interrupted, canceled, and missing-result behavior remains
  the M6G behavior.

## Kernel Plan

### Adapter Domain

- Generalize adapter compatibility so the required base capability comes from
  the caller instead of being hardcoded to `chat`.
- Add image-generation adapter support without breaking existing chat adapter
  metadata:
  - `target_capability = "image-generation"` or equivalent metadata.
  - `AdapterFormat::DiffusersLora` serialized as `diffusers-lora`.
  - `AdapterFormat::MlxDiffusionLora` serialized as `mlx-diffusion-lora`.
  - `AdapterBackendSupport::Diffusers` serialized as `diffusers`.
  - `AdapterBackendSupport::MlxDiffusion` serialized as `mlx-diffusion`.
  - Optional `trigger_words`.
  - Optional `recommended_scale`.
- Preserve old adapter records by defaulting absent `target_capability` to the
  legacy chat compatibility path when needed.

### Adapter Import And Pull

- Extend local import and Hugging Face pull requests with optional:
  - `target_capability`
  - `adapter_format`
  - `backend_support`
  - `weight_file`
  - `trigger_words`
  - `recommended_scale`
- Pass these options through CLI commands and daemon adapter import/pull routes,
  including direct mutations and detached job variants.
- Keep base-model binding conservative:
  - exact `base_model_ref` is strongest proof
  - otherwise matching source repo plus matching revision when both sides have
    revisions
  - otherwise reject live use with a clear compatibility error
- Detect common image LoRA source shapes:
  - `pytorch_lora_weights.safetensors`
  - `*.safetensors` when the source has exactly one candidate and the target
    capability is explicitly `image-generation`
  - repo-level metadata or README hints only as weak display metadata, not as
    compatibility proof
- If multiple `.safetensors` candidates exist, require an explicit adapter
  weight filename or reject with the candidate list.
- Store the selected runtime weight path relative to adapter `source/` when the
  source contains more than one possible weight file.

### Image Generation Domain

- Add an optional image LoRA selection to `ImageGenerationPreparationRequest`
  and the canonical runtime request:
  - adapter selector/ref
  - validated scale
  - resolved adapter metadata and source path
- Keep the base-model target separate from adapter metadata so logs, job
  summaries, and errors can mention both.
- Map image backends to adapter backend support:
  - `diffusers-text-to-image` -> `diffusers`
  - `mlx-diffusion-text-to-image` -> `mlx-diffusion`
- Reuse the adapter catalog and compatibility use case rather than opening the
  adapter store directly inside CLI or daemon code.

### Runtime Client

- Pass adapter arguments from Rust to the Python once entrypoint:
  - `--adapter-ref`
  - `--lora-scale`
  - resolved local adapter source path or selected weight path
- Avoid passing remote Hugging Face identifiers to runtime backends for managed
  adapters. Runtime execution should use Tentgent-managed local files.

## Python Runtime Plan

### Shared Runtime Types

- Extend `ImageGenerationRequest` or `ImageGenerationPlan` with an optional
  resolved adapter record:
  - `adapter_ref`
  - `adapter_source_dir`
  - selected `.safetensors` path when applicable
  - `lora_scale`
- Add a `select_adapter(adapter, scale)` method to `ImageGenerationBackend`,
  mirroring the existing chat backend shape.
- The default image backend implementation should reject non-empty adapter
  selection with a clear "adapter execution not implemented" error.

### Diffusers Backend

- Load the base pipeline exactly as M6G does.
- Resolve the adapter weight file from the managed adapter source.
- On the loaded pipeline instance, require LoRA-capable methods such as
  `load_lora_weights` and the best available scaling API for the installed
  Diffusers version.
- Apply the requested scale before generation.
- Release the pipeline after one-shot execution as today.
- If the pipeline class does not support LoRA, return an explicit runtime error
  naming the model backend and adapter ref.

### MFLUX Backend

- Pass local managed adapter weight paths through MFLUX `lora_paths`.
- Pass the requested scale through MFLUX `lora_scales`.
- Because MFLUX accepts LoRA at model construction time, select the adapter
  before the model instance is built or rebuild the model when selection changes.
- Do not let MFLUX perform its own remote LoRA download during Tentgent-managed
  execution.
- If no MFLUX-compatible public smoke fixture is available, keep the backend
  rejection explicit and update the roadmap before marking M6L complete.

## Daemon Plan

- Extend `ImageGenerationJobRequest` with:
  - `adapter_ref: Option<String>`
  - `lora_scale: Option<f32>`
- Resolve model and adapter before or at the start of the worker.
- Store adapter selection in job target/output metadata for operator visibility.
- Keep job result routes unchanged.
- Keep cancellation, stop-all-jobs, daemon stop, and workspace cleanup behavior
  unchanged.

## CLI Plan

- Extend `tentgent image generate` with:
  - `--adapter-ref <ADAPTER_REF>`
  - `--lora-scale <FLOAT>`
- Extend adapter commands with image LoRA metadata flags when needed:
  - `--target-capability image-generation`
  - `--adapter-format diffusers-lora|mlx-diffusion-lora`
  - `--backend-support diffusers|mlx-diffusion`
  - `--weight-file <RELATIVE_PATH>`
  - repeatable `--trigger-word`
  - optional `--recommended-scale`
- Update CLI help text so users understand that trigger words are prompt hints,
  not automatic prompt injection.

## Documentation Plan

- Update [adapter-store.md](../contracts/adapter-store.md) with image LoRA
  formats, backend-support names, trigger words, selected weight filename, and
  image-generation compatibility rules.
- Update [model-fixtures.md](../user/model-fixtures.md) with any verified small
  image LoRA fixtures and access notes.
- Update [commands.md](../user/commands.md) with adapter import/pull and image
  generation examples.
- Update [api.md](../user/api.md) with `adapter_ref` and `lora_scale` request
  fields for `POST /v1/images/generations/job`.
- Update runtime/backend docs if new environment hints are needed.

## Tests And Smoke

Required Rust tests:

- adapter format detection for image LoRA source layouts
- adapter metadata serialization with old-record defaults
- compatibility checks for `image-generation` instead of hardcoded `chat`
- CLI parsing for image `--adapter-ref` and `--lora-scale`
- daemon request JSON validation and error mapping
- image-generation preparation with compatible and incompatible adapters

Required Python tests:

- image-generation plan resolves adapter records and selected weight files
- Diffusers backend rejects pipelines without LoRA methods
- Diffusers backend calls LoRA load/scale hooks with a fake pipeline
- MFLUX backend passes local `lora_paths` and `lora_scales` with a fake model

Smoke tests:

1. Pull or import a small Diffusers-compatible text-to-image base model and one
   compatible image LoRA.
2. Run foreground CLI generation with `--adapter-ref`.
3. Run daemon `POST /v1/images/generations/job` with the same adapter.
4. Fetch the generated file through the existing image result route.
5. Repeat the same shape on the MFLUX backend if a compatible public fixture is
   available.

## Acceptance Criteria

- A user can import or pull an image LoRA into the managed adapter store.
- `adapter inspect` shows target capability, backend support, base model binding,
  trigger words when present, and selected weight-file information when needed.
- CLI text-to-image generation accepts one compatible image LoRA and writes the
  requested output file.
- Daemon image-generation jobs accept one compatible image LoRA and produce the
  normal result file list/download response.
- Incompatible adapter/base/backend combinations fail before expensive runtime
  work whenever possible.
- Diffusers image LoRA is implemented and covered by tests.
- MFLUX image LoRA is either implemented and smoke-tested, or explicitly blocked
  with a documented runtime/package/fixture reason before the roadmap status is
  changed.
- Existing no-adapter image generation continues to pass CLI and daemon smoke.

## Out Of Scope

- Multiple LoRA adapters in one request.
- LoRA stacking order or per-layer adapter composition.
- Image LoRA training.
- Automatic prompt rewriting from trigger words.
- OpenAI-compatible image APIs.
- Direct `tentgent server` image routes.
- Image-to-image, inpainting, masks, reference images, ControlNet, and upscaling.
- A global public compatibility registry.
