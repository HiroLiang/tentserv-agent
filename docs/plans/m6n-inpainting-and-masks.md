# M6N Inpainting And Masks

Status: implemented and unit-tested; real-model smoke pending.

M6N adds the first native masked image editing workflow: one base image, one
mask image, and one prompt produce one output image artifact. It extends the
image-generation feature family after M6M image-to-image without adding
reference images, ControlNet, automatic segmentation, or a drawing UI.

Depends on:

- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6G image generation jobs](./m6g-image-generation-jobs.md)
- [M6K MLX image generation backend decision](./m6k-mlx-image-generation-backend-decision.md)
- [M6L image generation LoRA](./m6l-image-generation-lora.md)
- [M6M image-to-image](./m6m-image-to-image.md)

Implementation references:

- Diffusers inpainting uses `AutoPipelineForInpainting`, an image, and a
  `mask_image`. White mask pixels are repainted; black mask pixels are kept:
  <https://huggingface.co/docs/diffusers/using-diffusers/inpaint>
- The installed MFLUX runtime exposes Flux Fill through
  `mflux.models.flux.variants.fill.flux_fill.Flux1Fill.generate_image(...)`
  with `image_path` and `masked_image_path` parameters.

## Goal

Allow users to repaint a masked area of one local image:

```bash
tentgent image inpaint \
  --model-ref <image-generation-model-ref> \
  --input-image input.png \
  --mask-image mask.png \
  --prompt "replace the marked area with a small ceramic teapot" \
  --strength 0.85 \
  --output inpainted.png
```

HTTP integrations should use a daemon multipart job route:

```http
POST /v1/images/inpaint/job
Content-Type: multipart/form-data
```

M6N should prove the mask contract, mask normalization, image/mask validation,
kernel workflow routing, Diffusers inpainting runtime path, MFLUX Flux Fill
runtime path when a compatible model is selected, daemon job workspace storage,
foreground CLI, and result-file download behavior.

## Scope

In scope:

- One base image.
- One mask image.
- One text prompt and optional negative prompt where the backend supports it.
- One output image file.
- Optional one image-generation LoRA adapter from M6L.
- Diffusers inpainting support.
- MFLUX Flux Fill support for compatible `mlx-diffusion` fill models.
- CLI foreground execution through kernel use cases.
- Daemon multipart upload to a job-owned workspace.

Out of scope:

- Automatic mask generation or segmentation.
- Drawing, editing, or preview UI for masks.
- Mask inversion flags.
- Mask blur, feathering, dilation, erosion, or other mask preprocessing knobs.
- Reference-image composition and ControlNet. That remains M6O.
- Multiple input images or multiple mask layers.
- Multiple output images or batch output.
- OpenAI-compatible image edits APIs.
- Direct `tentgent server` image routes.
- Multi-LoRA stacking, image LoRA training, or prompt auto-injection.
- Treating input images or masks as managed model/dataset/adapter objects.

## Product Decisions

- Keep `image-generation` as the model capability. Do not add a separate
  `image-inpainting` capability in M6N.
- Add a new CLI subcommand:
  `tentgent image inpaint`.
- Add a new native daemon route:
  `POST /v1/images/inpaint/job`.
- Use workflow-owned result routes aligned with M6G and M6M:
  - `GET /v1/images/inpaint/job/{job_id}/files`
  - `GET /v1/images/inpaint/job/{job_id}/files/{file_id}`
- Keep implementation helpers shared with existing image job routes where
  practical, but keep the public route distinct so users can see which workflow
  they are calling.
- Daemon input is multipart file bytes. It must not trust client-local paths.
- CLI input is local paths. CLI does not create daemon jobs.
- Uploaded base image and mask image are written to the job workspace before
  runtime execution starts. The model consumes complete local files, not
  partial streaming chunks.
- The daemon media upload cap applies independently to the `image` and `mask`
  file parts.
- Supported input image formats for M6N: PNG, JPEG, and WebP.
- Supported mask image formats for M6N: PNG, JPEG, and WebP. PNG is
  recommended because masks should preserve sharp binary regions.
- Supported output formats remain PNG and JPEG.

## Mask Semantics

Expose one public mask rule:

```text
white = repaint
black = keep
```

Rules:

- White or near-white mask pixels select the region to repaint.
- Black or near-black mask pixels preserve the original image region.
- Runtime normalization converts masks to 8-bit grayscale.
- Pixels with grayscale value `>= 128` become repaint pixels.
- Pixels with grayscale value `< 128` become keep pixels.
- M6N does not add `--invert-mask`. Users with inverse masks should invert
  before calling Tentgent.

Validation:

- Base image and mask must both exist for CLI.
- Base image and mask must both be uploaded for daemon.
- Both files must be non-empty.
- Both files must decode through Pillow before model loading.
- Base image and mask must have the same decoded dimensions before runtime
  resizing.
- Runtime may resize both base image and normalized mask to requested output
  dimensions together.
- Validation should happen before loading the diffusion model so mask mistakes
  fail quickly.

## Strength Semantics

Expose the same Diffusers-style public parameter as M6M:

```text
strength: 0.0..=1.0
```

Tentgent defines inpainting `strength` as denoising strength:

- `0.0` keeps the masked area close to the base image.
- `1.0` allows the masked area to be strongly regenerated.
- Default: `1.0`.

Runtime mapping:

- Diffusers receives `strength` directly.
- MFLUX Flux Fill receives `image_strength` only after confirming the installed
  runtime keeps the same input-image-influence semantic seen in M6M. If the
  semantic remains opposite, map `image_strength = 1.0 - strength`.
- If the MFLUX path cannot verify compatibility for the selected model, return
  a clear unsupported-backend error instead of guessing.

## CLI Contract

Add:

```bash
tentgent image inpaint \
  --model-ref <MODEL_REF> \
  --input-image <PATH> \
  --mask-image <PATH> \
  --prompt <TEXT> \
  --output <OUTPUT_PATH> \
  [--negative-prompt <TEXT>] \
  [--adapter-ref <ADAPTER_REF>] \
  [--lora-scale <FLOAT>] \
  [--strength <FLOAT>] \
  [--format png|jpg] \
  [--width <PX>] \
  [--height <PX>] \
  [--steps <N>] \
  [--guidance-scale <FLOAT>] \
  [--seed <N>] \
  [--home <HOME>]
```

Rules:

- `--input-image` must exist, be a regular file, be non-empty, and decode as
  PNG, JPEG, or WebP.
- `--mask-image` must exist, be a regular file, be non-empty, and decode as
  PNG, JPEG, or WebP.
- `--output` must not exist before execution.
- `--strength` defaults to `1.0` and must be finite in `0.0..=1.0`.
- `--adapter-ref` and `--lora-scale` follow M6L rules.
- `--width` and `--height` define the output dimensions. The runtime resizes
  both base image and normalized mask together when needed.
- On success, print only the output path summary already used by
  `image generate` and `image transform`.

## Daemon Contract

Create an inpaint job:

```http
POST /v1/images/inpaint/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | PNG, JPEG, or WebP base image. |
| `mask` | yes | file bytes | PNG, JPEG, or WebP mask image. White repaints; black keeps. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `prompt` | yes | string | Text prompt for the repaint area. |
| `negative_prompt` | no | string | Optional negative prompt where the backend supports it. |
| `adapter_ref` | no | string | Optional image LoRA adapter ref. |
| `lora_scale` | no | number | Optional LoRA scale. Requires `adapter_ref`. |
| `strength` | no | number | Defaults to `1.0`; must be `0.0..=1.0`. |
| `output_format` | no | string | `png`, `jpg`, or `jpeg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. |
| `width` | no | integer | Output width, same bounds as M6G. |
| `height` | no | integer | Output height, same bounds as M6G. |
| `steps` | no | integer | Same bounds as M6G. |
| `guidance_scale` | no | number | Same bounds as M6G. |
| `seed` | no | integer | Optional deterministic seed. |

Response shape:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<resolved-model-ref>"
    }
  }
}
```

Result behavior:

- Pending result files return `result_pending`.
- Failed jobs return the existing terminal job error mapping.
- File listing and download routes mirror M6G/M6M behavior under the
  `/v1/images/inpaint/job/{job_id}` route family.

## Kernel Plan

### Domain

- Keep the package under `features/image_generation`.
- Extend the workflow input type with an inpainting input:
  - base image path
  - base image media type
  - mask image path
  - mask image media type
  - inpaint strength
- Add or reuse a strength validation type:
  - default `1.0` for inpaint
  - finite `0.0..=1.0`
- Keep existing `ImageGenerationOptions` for dimensions, steps, guidance scale,
  and seed.
- Keep existing output format and response types.
- Keep one optional resolved image LoRA adapter on the request.

### Use Cases

- Extend `ImageGenerationPreparationRequest` with the new inpaint input.
- The model resolver should still require `image-generation` capability.
- The backend resolver should select a workflow-aware backend:
  - `diffusers-text-to-image`
  - `diffusers-image-to-image`
  - `diffusers-inpaint`
  - `mlx-diffusion-text-to-image`
  - `mlx-diffusion-image-to-image`
  - `mlx-diffusion-inpaint`
- Reuse M6L adapter compatibility. The required capability remains
  `image-generation`; backend support must match the selected runtime family.
- Existing text-to-image and image-to-image behavior must remain unchanged.

### Runtime Client

- Extend `PythonImageGenerationOnceRuntimeClient` arguments:
  - `--mask-image-path`
  - `--mask-image-media-type`
  - `--strength`
- Pass daemon workspace paths for daemon jobs.
- Pass CLI local paths for foreground CLI.
- Keep output path behavior unchanged.

## Python Runtime Plan

### Runtime Request

- Extend `ImageGenerationRequest` with:
  - `mask_image_path: Path | None`
  - `mask_image_media_type: str | None`
  - inpaint strength
- Infer workflow:
  - no input image: text-to-image
  - input image without mask: image-to-image
  - input image with mask: inpaint
- Reject mask without base image.
- Validate image path and mask path before model loading.
- Load image and mask with Pillow.
- Validate decoded dimensions match before resizing.
- Normalize mask to binary 8-bit grayscale with white repaint / black keep.
- Resize base image and normalized mask together to requested output
  dimensions when needed.

### Diffusers Backend

- Add a Diffusers inpainting backend path that uses
  `AutoPipelineForInpainting`.
- Keep text-to-image and image-to-image loading untouched.
- Apply one selected image LoRA adapter before generation, same as M6L/M6M.
- Call the inpainting pipeline with:
  - `prompt`
  - `negative_prompt`
  - `image`
  - `mask_image`
  - `strength`
  - `width`
  - `height`
  - `num_inference_steps`
  - `guidance_scale`
  - `generator`
- Return the same `ImageGenerationResult`.

### MFLUX Backend

- Add an MFLUX Flux Fill path with `Flux1Fill` when the selected model is
  compatible with fill/inpainting.
- Prefer the Python API over shelling out to `mflux-generate-fill`.
- Pass managed LoRA paths and scales exactly as in M6L when supported.
- Pass:
  - `image_path`
  - normalized `mask_image_path`
  - `prompt`
  - `width`
  - `height`
  - `num_inference_steps`
  - `guidance`
  - `seed`
  - mapped `image_strength`
- `Flux1Fill.generate_image` in the installed runtime does not expose
  `negative_prompt`. Reject `negative_prompt` for the MFLUX inpaint path with a
  clear unsupported option error in this slice.
- If support cannot be verified for the selected model, return a clear
  `backend_not_supported` style runtime error and keep the gap documented
  before marking M6N implemented.

## Daemon Plan

- Add multipart route `POST /v1/images/inpaint/job`.
- Reuse the media upload cap and multipart error style from M6D/M6F/M6M.
- Require exactly one `image` file part and exactly one `mask` file part.
- Write uploaded base image and mask bytes into the job workspace input area.
- Record input stream summary with media types, original filenames, byte count,
  and sha256 when the existing port can provide it.
- Spawn a daemon worker after both files are fully persisted.
- Reuse image result file declaration and download logic, preferably through
  shared helpers with M6G/M6M.
- Add route handlers for inpaint result file listing and download.
- Keep daemon result artifacts under the job workspace. Do not expose generic
  workspace or chunk internals.

## Documentation Plan

- Update [commands.md](../user/commands.md):
  - CLI `tentgent image inpaint`
  - mask semantics
  - daemon multipart curl example
  - result file listing and download
- Update [api.md](../user/api.md):
  - multipart route
  - fields and validation
  - mask semantics
  - pending/terminal result behavior
- Update [model-fixtures.md](../user/model-fixtures.md):
  - inpaint smoke commands
  - note which fixture is plumbing-only
  - note MFLUX fill smoke status separately
- Update [version.md](../user/version.md) after implementation.
- Update this roadmap entry after implementation.

## Tests

Implementation record:

- Added kernel image-generation workflow kind `inpaint`, including workflow-aware
  backend routing for Diffusers and MLX diffusion models.
- Added foreground CLI `tentgent image inpaint`.
- Added daemon multipart route `POST /v1/images/inpaint/job` plus workflow-owned
  result file listing and download routes.
- Added Python runtime request fields for `mask_image_path` and
  `mask_image_media_type`.
- Added Diffusers inpainting through `AutoPipelineForInpainting`.
- Added MFLUX Flux Fill routing through `Flux1Fill` with a compatibility guard
  that rejects non-fill-looking MLX diffusion models.
- Normalized masks to binary grayscale with `white = repaint` and
  `black = keep`.
- Added user documentation for CLI, daemon API, fixture commands, and version
  notes.

Verification run:

- `cargo test -p tentgent-kernel image_generation`
- `cargo test -p tentgent-cli image`
- `cargo test -p tentgent-daemon image_inpaint`
- `uv run python -m unittest tests.test_image_generation`
- `uv run --with ruff ruff check src tests`

Rust kernel:

- inpaint strength validation
- inpaint request preparation
- backend selection for text-to-image vs image-to-image vs inpaint
- adapter compatibility reused for inpaint
- Python runtime client passes `--mask-image-path`, `--mask-image-media-type`,
  and `--strength`

Rust CLI:

- parse `tentgent image inpaint`
- reject missing base image
- reject missing mask image
- reject existing output path
- reject invalid strength

Rust daemon:

- multipart route rejects missing image
- multipart route rejects missing mask
- multipart route rejects duplicate image or mask fields
- multipart route rejects invalid strength
- route creates job after both uploads persist
- result files mirror M6G/M6M pending and terminal behavior

Python:

- plan-only output includes input image, mask image, and strength
- mask without base image is rejected
- image/mask dimension mismatch is rejected before backend load
- normalized mask uses white repaint / black keep semantics
- Diffusers fake inpainting pipeline receives `image`, `mask_image`, and
  `strength`
- Diffusers fake LoRA path still applies with inpainting
- MFLUX fake Flux Fill path receives local image path, normalized mask path,
  and mapped image strength when supported
- MFLUX fake path rejects `negative_prompt`

Smoke:

- CLI smoke with a tiny Diffusers image-generation model, a small local input
  image, and a small binary mask.
- Daemon smoke with multipart `image` + `mask` upload and result download.
- Optional LoRA smoke only if a small public compatible fixture is already
  pinned.
- MFLUX smoke only if a practical local Flux Fill-compatible fixture is already
  present or already approved for download.

## Acceptance Criteria

- `tentgent image inpaint` writes one output image and never overwrites an
  existing file.
- `POST /v1/images/inpaint/job` accepts multipart base image and mask bytes and
  returns a job id.
- Inpaint result files can be listed and downloaded through workflow-owned
  routes.
- White mask pixels repaint and black mask pixels keep the original region.
- Image/mask decode and dimension errors fail before model loading.
- Diffusers inpainting has runtime wiring and unit coverage; repeatable
  real-model smoke remains pending until a practical inpainting fixture is
  pinned.
- MFLUX inpainting has Flux Fill runtime wiring and unit coverage with a
  compatibility guard; repeatable real-model smoke remains pending until a
  practical fill-compatible MLX fixture is pinned.
- Existing `tentgent image generate`, `tentgent image transform`,
  `/v1/images/generations/job`, and `/v1/images/transforms/job` behavior remain
  unchanged.
- M6L single-LoRA selection still works for text-to-image and image-to-image
  and is wired for inpainting when compatible.

## Deferred

- Mask inversion.
- Mask blur, feather, dilation, and erosion controls.
- Automatic segmentation and mask creation.
- Reference image composition.
- ControlNet.
- Multiple input images.
- Batch output or multiple generated images.
- OpenAI-compatible image edits API.
- Direct media server routes.
