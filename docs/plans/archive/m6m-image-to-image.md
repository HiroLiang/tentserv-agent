# M6M Image-To-Image

Status: implemented and unit-tested; real-model smoke pending.

M6M adds the first native image-to-image workflow: one input image plus one text
prompt produces one output image artifact. It extends the image-generation
feature family without adding masks, inpainting, reference-image composition, or
ControlNet.

Depends on:

- [M6G image generation jobs](./m6g-image-generation-jobs.md)
- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6K MLX image generation backend decision](./m6k-mlx-image-generation-backend-decision.md)
- [M6L image generation LoRA](./m6l-image-generation-lora.md)

Implementation references:

- Diffusers image-to-image uses an image input and a `strength` parameter:
  <https://huggingface.co/docs/diffusers/using-diffusers/img2img>
- MFLUX exposes image-to-image through `--image-path` and `--image-strength`:
  <https://pypi.org/project/mflux/0.12.1/>

## Goal

Allow users to transform one local image with a prompt:

```bash
tentgent image transform \
  --model-ref <image-generation-model-ref> \
  --input-image input.png \
  --prompt "turn this into a watercolor illustration" \
  --strength 0.6 \
  --output output.png
```

HTTP integrations should use a daemon multipart job route:

```http
POST /v1/images/transforms/job
Content-Type: multipart/form-data
```

M6M should prove the input-image contract, strength semantics, kernel request
shape, Diffusers image-to-image runtime path, MFLUX image-to-image path when
practical, daemon job workspace storage, foreground CLI, and result-file
download behavior.

## Implementation Record

- Added kernel workflow typing for text-to-image and image-to-image requests.
- Added `tentgent image transform` for foreground one-input-image transforms.
- Added daemon `POST /v1/images/transforms/job` as a multipart upload route
  that persists uploaded image bytes into the job workspace before worker
  execution.
- Added transform result file routes:
  - `GET /v1/images/transforms/job/{job_id}/files`
  - `GET /v1/images/transforms/job/{job_id}/files/{file_id}`
- Added Diffusers image-to-image runtime loading through
  `AutoPipelineForImage2Image`.
- Added MFLUX image-to-image argument mapping through the existing
  `generate_image(..., image_path, image_strength)` API.
- Added validation and tests for Diffusers-style public `strength`.
- Updated user CLI/API/fixture/version documentation.
- Real-model smoke remains pending because available local fixtures and
  installed runtime dependencies vary by machine.

## Scope

In scope:

- One input image.
- One text prompt and optional negative prompt.
- One output image file.
- Optional one image-generation LoRA adapter from M6L.
- Diffusers image-to-image support.
- MFLUX image-to-image support if the installed runtime exposes a stable Python
  or CLI path that can consume Tentgent-managed local model files.
- CLI foreground execution through kernel use cases.
- Daemon multipart upload to a job-owned workspace.

Out of scope:

- Masks and inpainting. That remains M6N.
- Reference-image composition and ControlNet. That remains M6O.
- Multiple input images.
- Multiple output images.
- OpenAI-compatible image edits APIs.
- Direct `tentgent server` image routes.
- Prompt auto-injection, image LoRA training, or multi-LoRA stacking.
- Treating input images as managed model/dataset/adapter objects.

## Product Decisions

- Keep `image-generation` as the model capability. Do not add a separate
  `image-to-image` capability in M6M.
- Add a new CLI subcommand instead of overloading text-to-image generation:
  `tentgent image transform`.
- Add a new native daemon route:
  `POST /v1/images/transforms/job`.
- Use workflow-owned result routes aligned with M6G:
  - `GET /v1/images/transforms/job/{job_id}/files`
  - `GET /v1/images/transforms/job/{job_id}/files/{file_id}`
- Keep implementation helpers shared with M6G where practical, but keep the
  public route distinct so users can see which workflow they are calling.
- Daemon input is multipart file bytes. It must not trust client-local paths.
- CLI input is a local path. CLI does not create daemon jobs.
- The uploaded input image is written to the job workspace before runtime
  execution starts. The model consumes a complete local file, not partial
  streaming chunks.
- The daemon media upload cap applies to the `image` file part.
- Supported input image formats for M6M: PNG, JPEG, and WebP.
- Supported output formats remain PNG and JPEG.

## Strength Semantics

Expose one public parameter:

```text
strength: 0.0..=1.0
```

Tentgent defines `strength` with Diffusers-compatible denoising semantics:

- `0.0` means preserve the input image as much as possible.
- `1.0` means the model may largely ignore the input image.
- Default: `0.6`.

Runtime mapping:

- Diffusers receives `strength` directly.
- MFLUX documents `image_strength` as input-image influence where `0.0` means
  no influence. If the installed MFLUX API keeps that opposite semantic, map:
  `image_strength = 1.0 - strength`.
- If the MFLUX Python API semantics cannot be verified, keep the MFLUX path
  blocked with a clear runtime error instead of guessing.

## CLI Contract

Add:

```bash
tentgent image transform \
  --model-ref <MODEL_REF> \
  --input-image <PATH> \
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

- `--input-image` must exist, be a regular file, and decode as PNG, JPEG, or
  WebP.
- `--output` must not exist before execution.
- `--strength` defaults to `0.6` and must be finite in `0.0..=1.0`.
- `--adapter-ref` and `--lora-scale` follow M6L rules.
- `--width` and `--height` define the output dimensions. The runtime may resize
  the input image to match these dimensions before generation.
- On success, print only the output path summary already used by
  `image generate`.

## Daemon Contract

Create a transform job:

```http
POST /v1/images/transforms/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | PNG, JPEG, or WebP input image. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `prompt` | yes | string | Text prompt. Blank prompts are rejected. |
| `negative_prompt` | no | string | Optional negative prompt. |
| `adapter_ref` | no | string | Optional image LoRA adapter ref. |
| `lora_scale` | no | number | Optional LoRA scale. Requires `adapter_ref`. |
| `strength` | no | number | Defaults to `0.6`; must be `0.0..=1.0`. |
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
- File listing and download routes mirror M6G behavior under the
  `/v1/images/transforms/job/{job_id}` route family.

## Kernel Plan

### Domain

- Keep the package under `features/image_generation`.
- Add an input mode or source type:
  - text-to-image with no input image
  - image-to-image with input image path, media type, and strength
- Add `ImageTransformStrength` or equivalent validation type:
  - default `0.6`
  - finite `0.0..=1.0`
- Keep existing `ImageGenerationOptions` for dimensions, steps, guidance scale,
  and seed.
- Keep existing output format and response types.
- Keep one optional resolved image LoRA adapter on the request.

### Use Cases

- Extend `ImageGenerationPreparationRequest` with optional image-to-image input
  metadata, or introduce a parallel preparation request that reuses the same
  runtime client.
- The model resolver should still require `image-generation` capability.
- The backend resolver should select a workflow-aware backend:
  - `diffusers-text-to-image`
  - `diffusers-image-to-image`
  - `mlx-diffusion-text-to-image`
  - `mlx-diffusion-image-to-image`
- Reuse M6L adapter compatibility. The required capability remains
  `image-generation`; the backend support must match the selected runtime
  family.

### Runtime Client

- Extend `PythonImageGenerationOnceRuntimeClient` arguments:
  - `--input-image-path`
  - `--strength`
- Pass the input image workspace path for daemon jobs.
- Pass the CLI input image path for foreground CLI.
- Keep output path behavior unchanged.

## Python Runtime Plan

### Runtime Request

- Extend `ImageGenerationRequest` with:
  - `input_image_path: Path | None`
  - `strength: float | None`
- Validate:
  - image path exists
  - image path is a regular file
  - strength is present only when input image is present
  - strength is finite in `0.0..=1.0`
- Load and normalize the input image with Pillow.
- Resize or convert the input image consistently before backend execution:
  - convert to RGB for JPEG output compatibility
  - resize to requested output dimensions when required by backend

### Diffusers Backend

- Add a Diffusers image-to-image backend path that uses
  `AutoPipelineForImage2Image` or the equivalent local pipeline class.
- Keep text-to-image loading untouched for M6G.
- Apply one selected image LoRA adapter before generation, same as M6L.
- Call the image-to-image pipeline with:
  - `prompt`
  - `negative_prompt`
  - `image`
  - `strength`
  - `width`
  - `height`
  - `num_inference_steps`
  - `guidance_scale`
  - `generator`
- Return the same `ImageGenerationResult`.

### MFLUX Backend

- Add an MFLUX image-to-image path if the installed runtime exposes a stable
  API for local model paths and local input image paths.
- Prefer a Python API over shelling out to `mflux-generate`.
- If only CLI support is stable, document the tradeoff before implementation.
- Pass managed LoRA paths and scales exactly as in M6L.
- Map Tentgent `strength` to MFLUX `image_strength` only after confirming the
  runtime semantic. If semantic remains opposite, use `1.0 - strength`.
- If support cannot be verified in this slice, return a clear
  `backend_not_supported` style runtime error for MFLUX image-to-image and
  update the roadmap before marking M6M implemented.

## Daemon Plan

- Add multipart route `POST /v1/images/transforms/job`.
- Reuse the media upload cap and multipart error style from M6D/M6F.
- Write uploaded input image bytes into the job workspace input area.
- Record input stream summary with media type, original filename, byte count,
  and sha256 when the existing port can provide it.
- Spawn a daemon worker after the input file is fully persisted.
- Reuse image result file declaration and download logic, preferably through
  shared helpers with M6G.
- Add route handlers for transform result file listing and download.
- Keep daemon result artifacts under the job workspace. Do not expose generic
  workspace or chunk internals.

## Documentation Plan

- Update [commands.md](../../user/commands.md):
  - CLI `tentgent image transform`
  - daemon multipart curl example
  - result file listing and download
- Update [api.md](../../user/api.md):
  - multipart route
  - fields and validation
  - pending/terminal result behavior
- Update [model-fixtures.md](../../user/model-fixtures.md):
  - image-to-image smoke commands
  - note whether the tiny Diffusers fixture is only plumbing quality
  - note MFLUX smoke status separately
- Update this roadmap entry after implementation.

## Tests

Rust kernel:

- strength validation
- image-to-image request preparation
- backend selection for text-to-image vs image-to-image
- adapter compatibility reused for image-to-image
- Python runtime client passes `--input-image-path` and `--strength`

Rust CLI:

- parse `tentgent image transform`
- reject missing input file
- reject existing output path
- reject invalid strength

Rust daemon:

- multipart route rejects missing image
- multipart route rejects invalid strength
- route creates job after upload persists
- result files mirror M6G pending and terminal behavior

Python:

- plan-only output includes input image path and strength
- Diffusers fake image-to-image pipeline receives `image` and `strength`
- Diffusers fake LoRA path still applies with image-to-image
- MFLUX fake path receives local image path and mapped strength when supported

Smoke:

- CLI smoke with a tiny Diffusers image-generation model and a small local input
  image.
- Daemon smoke with multipart upload and result download.
- Optional LoRA smoke only if a small public compatible fixture is already
  pinned.
- MFLUX smoke only if a practical local fixture is already present or already
  approved for download.

## Acceptance Criteria

- `tentgent image transform` writes one output image and never overwrites an
  existing file.
- `POST /v1/images/transforms/job` accepts multipart image bytes and returns a
  job id.
- Transform result files can be listed and downloaded through workflow-owned
  routes.
- Diffusers image-to-image works with a fake runtime test and at least one tiny
  smoke fixture.
- MFLUX image-to-image is either implemented and smoke-tested, or explicitly
  blocked with a documented runtime/API reason before status changes.
- Existing `tentgent image generate` and `/v1/images/generations/job` behavior
  remain unchanged.
- M6L single-LoRA selection still works for text-to-image and is wired for
  image-to-image when compatible.

## Deferred

- Inpainting and mask semantics.
- Reference image composition.
- ControlNet.
- Multiple input images.
- Batch output or multiple generated images.
- OpenAI-compatible image edits API.
- Direct media server routes.
