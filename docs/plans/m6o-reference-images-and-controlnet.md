# M6O Reference Images And ControlNet

Status: implemented, unit-tested, and smoke-tested with a tiny Diffusers
ControlNet fixture pair.

M6O adds the first typed controlled image-generation workflow after text-to-image,
image-to-image, LoRA, and inpainting are stable. The slice should not expose
job-workspace plumbing, generic upload management, or an ambiguous
`reference_image` field. It should expose one product workflow: generate an
image from a prompt plus a typed control image.

This plan intentionally separates two ideas that are often called "reference
image" in image tools:

- Already implemented image inputs:
  - image-to-image transforms in M6M
  - masked inpainting in M6N
- Typed controlled generation:
  - ControlNet-style control images, such as canny, depth, pose, or line art
- Deferred reference-image composition:
  - IP-Adapter, style reference, identity reference, image-prompt blending, and
    other backend-specific composition features

M6O should implement the typed controlled-generation path first. Generic
reference-image composition remains out of scope until Tentgent has a concrete
backend contract and compatibility proof for that reference family.

## Implementation Record

- Added managed ControlNet-style adapter metadata:
  - `adapter_type = "controlnet"`
  - `adapter_format = "diffusers-controlnet"`
  - `control_kind = "canny"`
- Added foreground CLI `tentgent image control`.
- Added daemon multipart route `POST /v1/images/control/job`.
- Added workflow result routes:
  - `GET /v1/images/control/job/{job_id}/files`
  - `GET /v1/images/control/job/{job_id}/files/{file_id}`
- Added kernel image-generation control workflow typing and runtime request
  routing.
- Added Python Diffusers ControlNet runtime wiring through managed local
  ControlNet adapter source directories.
- Added explicit unsupported-backend behavior for MFLUX image control.
- Updated user/API/model-fixture/version and adapter-store documentation.
- Smoke-tested daemon `POST /v1/images/control/job` with
  `hf-internal-testing/tiny-stable-diffusion-pipe-no-safety` plus
  `hf-internal-testing/tiny-controlnet` at `64x64`, `2` steps.

Depends on:

- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6G image generation jobs](./m6g-image-generation-jobs.md)
- [M6K MLX image generation backend decision](./m6k-mlx-image-generation-backend-decision.md)
- [M6L image generation LoRA](./m6l-image-generation-lora.md)
- [M6M image-to-image](./m6m-image-to-image.md)
- [M6N inpainting and masks](./m6n-inpainting-and-masks.md)

## Goal

Allow users to generate one image from one text prompt plus one typed control
image:

```bash
tentgent image control \
  --model-ref <image-generation-model-ref> \
  --control-ref <controlnet-adapter-ref> \
  --control-image control.png \
  --control-kind canny \
  --prompt "a small cabin in a winter forest" \
  --output controlled.png
```

HTTP integrations should use a daemon multipart job route:

```http
POST /v1/images/control/job
Content-Type: multipart/form-data
```

M6O should prove the control-image request shape, control-asset compatibility,
kernel routing, Diffusers ControlNet runtime path, daemon job workspace storage,
foreground CLI, and result-file download behavior. It should also document why
generic reference-image composition is not a single stable API yet.

## Scope

In scope:

- One base image-generation model.
- One typed control image file.
- One managed ControlNet-style control asset referenced by `--control-ref`.
- One text prompt and optional negative prompt.
- One output image file.
- Optional one image-generation LoRA adapter from M6L.
- Diffusers ControlNet support for the first backend path.
- CLI foreground execution through kernel use cases.
- Daemon multipart upload to a job-owned workspace.
- Clear unsupported-backend errors for MLX/MFLUX when no stable local
  ControlNet path is available.

Out of scope:

- A generic `--reference-image` option.
- IP-Adapter, style reference, identity reference, and image-prompt composition.
- Automatic canny/depth/pose preprocessing unless approved as a follow-up.
- Multiple control images.
- Multi-ControlNet stacking.
- Control weight schedules or per-block control weights.
- OpenAI-compatible image edits APIs.
- Direct `tentgent server` image routes.
- Image LoRA training.
- ControlNet training or conversion.
- Treating uploaded control images as managed model, dataset, or adapter
  objects.

## Product Decisions

- Keep `image-generation` as the model capability. Do not add a separate
  `controlnet` model capability in M6O.
- Add a new CLI subcommand:
  `tentgent image control`.
- Add a new native daemon route:
  `POST /v1/images/control/job`.
- Use workflow-owned result routes aligned with previous image slices:
  - `GET /v1/images/control/job/{job_id}/files`
  - `GET /v1/images/control/job/{job_id}/files/{file_id}`
- Use `--control-ref` for the managed ControlNet-style control asset.
- Keep `--adapter-ref` reserved for the optional image LoRA adapter.
- Treat ControlNet as a control adapter/control asset, not as the base image
  model. It must be compatible with the selected base model and backend.
- Daemon input is multipart file bytes. It must not trust client-local paths.
- CLI input is a local path. CLI does not create daemon jobs.
- Uploaded control images are written to the job workspace before runtime
  execution starts. The model consumes a complete local file, not partial
  streaming chunks.
- The daemon media upload cap applies to the `control_image` file part.
- Supported control image formats for M6O: PNG, JPEG, and WebP.
- Supported output formats remain PNG and JPEG.

## Reference Image Boundary

Do not add a generic reference-image API in M6O.

The phrase "reference image" can mean different runtime contracts:

- image-to-image: one source image is denoised into a new image
- inpainting: one source image plus one mask is partially regenerated
- ControlNet: one control image conditions generation through a separate
  control model
- IP-Adapter or similar: one or more reference images influence style,
  composition, or identity through a separate adapter family
- multimodal prompt image: an image is interpreted by a vision-language model
  rather than an image generator

Tentgent should expose each of those as a typed workflow when its input/output
contract is clear. M6O only adds the ControlNet-style typed workflow.

## Control Asset Contract

Extend managed adapters so a control asset can be stored beside LoRA adapters
without being treated as a LoRA:

- `adapter_type = "controlnet"`
- `adapter_format = "diffusers-controlnet"`
- `target_capability = "image-generation"`
- `backend_support = ["diffusers"]`
- `control_kind = "canny"` for the first supported kind
- optional `base_model_ref`
- optional `base_model_source_repo`
- optional `base_model_source_revision`
- optional `model_family`
- optional source provenance fields already used by adapters

Compatibility checks before execution:

- The control asset must exist in the managed adapter store.
- `adapter_type` must be `controlnet`.
- `target_capability` must be `image-generation`.
- `backend_support` must include the selected image backend.
- `control_kind` must match the request.
- Exact `base_model_ref` is strongest proof when present.
- Otherwise source repo/revision or model family can be used only when the
  existing compatibility rules say that proof is strong enough.
- If compatibility cannot be proven, reject the request before model loading.

This keeps the base model, optional LoRA adapter, and control asset separate:

```text
base image model       -> --model-ref
optional image LoRA    -> --adapter-ref
control asset/model    -> --control-ref
uploaded control image -> --control-image or multipart control_image
```

## Control Image Semantics

M6O should expose one public control-strength value:

```text
control_strength: 0.0..=2.0
```

Default: `1.0`.

Rules:

- `0.0` means the control image has no practical influence.
- `1.0` is the backend default strength.
- Values above `1.0` make the control signal stronger when the backend supports
  it.
- Runtime maps this value to Diffusers `controlnet_conditioning_scale`.

The first implementation should require the uploaded image to already be a
valid control image for the selected `control_kind`. Automatic preprocessors
such as canny edge extraction or pose detection should be added later only
after their dependencies and model-specific behavior are approved.

## CLI Contract

Add:

```bash
tentgent image control \
  --model-ref <MODEL_REF> \
  --control-ref <CONTROL_ADAPTER_REF> \
  --control-image <PATH> \
  --control-kind canny \
  --prompt <TEXT> \
  --output <OUTPUT_PATH> \
  [--negative-prompt <TEXT>] \
  [--adapter-ref <LORA_ADAPTER_REF>] \
  [--lora-scale <FLOAT>] \
  [--control-strength <FLOAT>] \
  [--format png|jpg] \
  [--width <PX>] \
  [--height <PX>] \
  [--steps <N>] \
  [--guidance-scale <FLOAT>] \
  [--seed <N>]
```

Rules:

- `--control-ref` accepts a full adapter ref or unique short-ref prefix.
- `--adapter-ref` keeps its M6L meaning: optional image LoRA.
- `--lora-scale` is valid only when `--adapter-ref` is present.
- `--control-kind` is required in M6O and initially supports `canny`.
- `--output` must not already exist.
- CLI validates the local control image before invoking runtime execution.
- On success, CLI writes only the requested output file and prints a concise
  completion summary.
- On failure, errors should distinguish missing control asset, ambiguous
  control ref, incompatible control asset, unsupported backend, invalid control
  image, and runtime failure.

## Daemon Contract

Add:

```http
POST /v1/images/control/job
Content-Type: multipart/form-data
```

Required multipart fields:

- `control_image`: file bytes
- `model_ref`
- `control_ref`
- `control_kind`
- `prompt`

Optional multipart fields:

- `negative_prompt`
- `adapter_ref`
- `lora_scale`
- `control_strength`
- `output_format`
- `output_filename`
- `width`
- `height`
- `steps`
- `guidance_scale`
- `seed`

Rules:

- The daemon persists `control_image` into the job workspace before starting
  the worker.
- The daemon never accepts a client-local control-image path.
- If the upload exceeds the configured media cap, return a clear request error.
- If the job has not completed, result-file routes return the same explicit
  not-ready behavior used by previous image job slices.
- Completed result files are exposed only through workflow-owned file routes:
  - `GET /v1/images/control/job/{job_id}/files`
  - `GET /v1/images/control/job/{job_id}/files/{file_id}`
- Generic job routes remain status/control surfaces only.

## Kernel Plan

### Image Generation Domain

- Add `ImageGenerationWorkflowKind::Control`.
- Add `ImageControlKind` with `canny` as the first variant.
- Add `ImageControlStrength` validation.
- Add a resolved control asset record separate from the optional LoRA adapter:
  - control adapter ref
  - backend support
  - source path
  - control kind
- Add `ImageGenerationInput::Control`:
  - `control_image_path`
  - `control_media_type`
  - `control_kind`
  - `control_strength`
- Keep existing `TextToImage`, `ImageToImage`, and `Inpaint` request shapes
  stable.

### Adapter Domain

- Add a non-LoRA adapter type for ControlNet-style control assets.
- Add a Diffusers ControlNet adapter format.
- Add optional control-kind metadata to adapter records.
- Keep LoRA scale and trigger-word metadata LoRA-specific.
- Reuse adapter store identity, source indexing, local import, and Hugging Face
  pull flows where possible.

### Preparation Use Case

- Resolve the base image model exactly as current image-generation requests do.
- Resolve optional image LoRA exactly as M6L does.
- Resolve `control_ref` through adapter catalog/compatibility use cases.
- Reject unsupported combinations before runtime execution.
- Produce one canonical runtime request containing base model, optional LoRA,
  control asset, control image path, and output path.

## Python Runtime Plan

### Shared Runtime Types

- Extend the image generation runtime request with an optional controlled input
  record:
  - `control_image_path`
  - `control_kind`
  - `control_strength`
  - `control_adapter_ref`
  - `control_adapter_source_dir`
- Keep the optional LoRA adapter record separate.
- The default image backend should reject controlled input with a clear
  unsupported-workflow error unless the backend implements it.

### Diffusers Backend

- Load the managed base pipeline from local model files.
- Load the managed ControlNet asset from local adapter files.
- Build the appropriate Diffusers ControlNet pipeline.
- Decode the control image with Pillow and resize/normalize as required by the
  pipeline.
- Apply optional LoRA after the pipeline is constructed.
- Pass `controlnet_conditioning_scale = control_strength`.
- Save exactly one output image to the requested output path.
- Return explicit errors when:
  - the selected model is not compatible with ControlNet
  - the control asset cannot be loaded
  - the pipeline class is unavailable in the installed Diffusers version
  - the control image cannot be decoded

### MLX/MFLUX Backend

- Do not guess a ControlNet implementation for MLX/MFLUX.
- If a stable local ControlNet-capable MLX diffusion API is verified during
  implementation, add it behind the same `tentgent image control` contract.
- Otherwise return a clear unsupported-backend error and keep the gap recorded
  for the post-M7 compatibility architecture work.

## Daemon Plan

- Add a multipart parser for `control_image` plus text fields.
- Reuse existing image job workspace and result-file helpers.
- Keep control-image persistence workflow-owned; do not expose a generic upload
  or spool API.
- Add a worker branch for controlled generation.
- Include control target metadata in job status summaries where useful:
  - base model ref
  - control ref
  - control kind
  - optional LoRA adapter ref
- Preserve daemon stop behavior from prior job slices: stop running jobs, mark
  interrupted/failed jobs, and leave cleanup buffer time before GC.

## Documentation Plan

Update user/developer docs after implementation:

- `docs/user/commands.md`
  - CLI `tentgent image control` examples
  - multipart daemon examples
  - result-file fetch examples
- `docs/user/model-fixtures.md`
  - recommended small base image model plus ControlNet fixture when pinned
  - gated or license-required warnings where needed
- `docs/user/version.md`
  - implemented M6O feature and known limits
- `docs/contracts/adapter-store.md`
  - control adapter metadata
  - compatibility rules
- API documentation files that list daemon image routes.

Real-model smoke is complete for the tiny Diffusers fixture pair:

- Base model: `hf-internal-testing/tiny-stable-diffusion-pipe-no-safety`
- Control asset: `hf-internal-testing/tiny-controlnet`
- Adapter metadata:
  - `adapter_type = "controlnet"`
  - `adapter_format = "diffusers-controlnet"`
  - `backend_support = "diffusers"`
  - `control_kind = "canny"`
- Smoke settings: `width = 64`, `height = 64`, `steps = 2`, `seed = 1`
- Result path verified through
  `GET /v1/images/control/job/{job_id}/files/{file_id}`

## Test Plan

Rust:

- Kernel domain tests for:
  - `control_kind` parsing
  - `control_strength` validation
  - controlled workflow backend routing
  - control asset compatibility rejection
- Adapter store tests for:
  - `controlnet` adapter type
  - `diffusers-controlnet` adapter format
  - control-kind metadata round trip
- CLI tests for:
  - argument parsing
  - missing output protection
  - `--lora-scale` requiring `--adapter-ref`
  - short-ref resolution for `--control-ref`
- Daemon tests for:
  - multipart `control_image` persistence
  - queued job response
  - missing result not-ready behavior
  - result file list/download behavior

Python:

- Unit tests for runtime request parsing.
- Fake Diffusers backend test proving:
  - ControlNet source path is passed
  - control image path is passed
  - `controlnet_conditioning_scale` receives control strength
  - optional LoRA remains independent
- Unsupported backend test for controlled input when backend support is missing.

Smoke:

- Daemon smoke against the pinned tiny Diffusers ControlNet pair.
- Verify job success and result download through the workflow result route.
- Keep this fixture documented as plumbing-only; do not use it for output
  quality expectations.

## Acceptance Criteria

- `tentgent image control` exists and writes one output image for a compatible
  local Diffusers base model plus managed ControlNet asset.
- `POST /v1/images/control/job` accepts multipart file bytes and returns a job.
- Result files are fetched through image-control workflow routes.
- Generic job routes remain status/control only.
- ControlNet assets are represented as managed control adapters, not as base
  image models and not as LoRA adapters.
- Incompatible base model/control asset/backend combinations fail before model
  loading when metadata is sufficient to know they cannot work.
- Existing image generation, LoRA, image-to-image, and inpainting commands and
  routes remain compatible.
- Generic reference-image composition is documented as deferred, not silently
  collapsed into ControlNet.

## Execution Order

1. Extend adapter metadata for managed ControlNet-style control assets.
2. Add kernel domain types and compatibility preparation for controlled image
   generation.
3. Add foreground CLI `tentgent image control`.
4. Add Python Diffusers ControlNet runtime request support.
5. Add daemon multipart `POST /v1/images/control/job` and result routes.
6. Update user, API, fixture, version, and adapter-store documentation.
7. Run unit tests and, if a practical fixture is pinned, one real-model smoke.
