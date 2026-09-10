# Image Transform, Inpaint, And Control

Choose image-to-image transformation, mask-based repainting, or ControlNet guidance. Import a model and any compatible adapters before running. See [image prerequisites](./README.md) and [media file rules](../README.md#file-and-http-media-rules).

## Examples And Common Operations

### Transform

Transform one input image with a prompt:

```bash
tentgent image transform \
  --model-ref <image-generation-model-ref> \
  --input-image input.png \
  --prompt "Turn this into a watercolor illustration" \
  --strength 0.6 \
  --output transformed.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

`tentgent image transform` is foreground-only like `image generate`: it reads
the local `--input-image`, writes only to `--output`, and fails if the output
file already exists. Input images must be PNG, JPEG, or WebP. `--strength`
uses Diffusers image-to-image semantics: `0.0` preserves the input image as
much as possible, while `1.0` lets the model regenerate most of the image.
The same optional `--adapter-ref` and `--lora-scale` flags work for compatible
image LoRA adapters.

### Inpaint

Repaint only the white area of one mask image:

```bash
tentgent image inpaint \
  --model-ref <image-generation-model-ref> \
  --input-image input.png \
  --mask-image mask.png \
  --prompt "Replace the masked area with a small ceramic teapot" \
  --strength 1.0 \
  --output inpainted.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

`tentgent image inpaint` is foreground-only like the other image commands. The
base image and mask must be PNG, JPEG, or WebP files. Mask semantics are
`white = repaint` and `black = keep`; Tentgent normalizes the mask to binary
grayscale before runtime execution. The input image and mask must decode to
the same dimensions before the runtime resizes them to the requested output
size. `--strength` defaults to `1.0`, must be `0.0..=1.0`, and uses the same
Diffusers-style denoising meaning as `image transform`.

MLX inpainting requires a Flux Fill-compatible `mlx-diffusion` model; general
Flux text-to-image models are rejected for this workflow instead of guessed
through an incompatible runtime path.

### Control

Generate from a prompt plus one typed control image:

```bash
tentgent image control \
  --model-ref <image-generation-model-ref> \
  --control-ref <controlnet-adapter-ref> \
  --control-image control.png \
  --control-kind canny \
  --prompt "A small cabin following the control image structure" \
  --control-strength 1.0 \
  --output controlled.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

`tentgent image control` is foreground-only. It reads the local
`--control-image`, resolves `--control-ref` as a managed ControlNet-style
adapter, and writes only to `--output`. M6O supports `--control-kind canny`.
The control image must already be the control representation for that kind;
Tentgent does not auto-run canny/depth/pose preprocessing in this slice.
`--control-strength` defaults to `1.0` and must be `0.0..=2.0`.
Optional image LoRA still uses `--adapter-ref` and `--lora-scale`.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `image transform/inpaint/control` | `-m, --model-ref <MODEL_REF>` | Stored Tentgent image-generation model reference to run |
| `image transform` | `-i, --input-image <PATH>` | Local input image path. PNG, JPEG, and WebP are supported |
| `image transform` | `-p, --prompt <TEXT>` | Prompt describing the requested transformation |
| `image transform/inpaint/control` | `--negative-prompt <TEXT>` | Optional negative prompt |
| `image transform/inpaint/control` | `--adapter-ref <ADAPTER_REF>` | Optional managed image LoRA adapter reference |
| `image transform/inpaint/control` | `--lora-scale <FLOAT>` | Optional LoRA scale. Requires --adapter-ref |
| `image transform` | `--strength <FLOAT>` | Diffusers-style denoising strength. 0 preserves input most; 1 regenerates most [default: 0.6] |
| `image transform/inpaint/control` | `-o, --output <OUTPUT_PATH>` | Local output path. Existing files are never overwritten |
| `image transform/inpaint/control` | `--format <FORMAT>` | Output image format intent: png or jpg [default: png] |
| `image transform/inpaint/control` | `--width <PX>` | Output image width in pixels. Must be 64..1024 and divisible by 8 [default: 512] |
| `image transform/inpaint/control` | `--height <PX>` | Output image height in pixels. Must be 64..1024 and divisible by 8 [default: 512] |
| `image transform/inpaint/control` | `--steps <N>` | Diffusion inference steps. Must be 1..100 [default: 20] |
| `image transform/inpaint/control` | `--guidance-scale <FLOAT>` | Classifier-free guidance scale. Must be 0..30 [default: 7.5] |
| `image transform/inpaint/control` | `--seed <N>` | Optional deterministic seed |
| `image transform/inpaint/control` | `-H, --home <HOME>` | Optional Tentgent runtime home override |
| `image inpaint` | `-i, --input-image <PATH>` | Local base image path. PNG, JPEG, and WebP are supported |
| `image inpaint` | `--mask-image <PATH>` | Local mask image path. White pixels repaint; black pixels keep |
| `image inpaint` | `-p, --prompt <TEXT>` | Prompt describing the requested repaint |
| `image inpaint` | `--strength <FLOAT>` | Diffusers-style denoising strength. 0 preserves masked area most; 1 repaints most [default: 1] |
| `image control` | `--control-ref <ADAPTER_REF>` | Managed ControlNet-style adapter reference |
| `image control` | `--control-image <PATH>` | Local control image path. PNG, JPEG, and WebP are supported |
| `image control` | `--control-kind <KIND>` | Control image kind. M6O supports canny [default: canny] |
| `image control` | `-p, --prompt <TEXT>` | Prompt for the generated image |
| `image control` | `--control-strength <FLOAT>` | Control influence strength. 0 disables control; 1 is the backend default [default: 1] |

## HTTP API

Start the [daemon](../../daemon.md) and follow the [HTTP authentication and error rules](../../api.md).

### Image Transform Jobs

Canonical image-to-image transform uses a workflow job:

```http
POST /v1/images/transforms/job
Content-Type: multipart/form-data
```

The request uploads image bytes, not a client-local path. In curl,
`-F image=@/absolute/path/input.png` is client-side shorthand for reading that
file and sending bytes to the daemon.

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | PNG, JPEG, or WebP input image. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt describing the transform. |
| `negative_prompt` | no | string | Optional negative prompt. |
| `strength` | no | number | Defaults to `0.6`. Must be 0..1. `0.0` preserves input most; `1.0` regenerates most. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

The daemon persists the uploaded image into the job workspace before the worker
starts. Diffusers image-to-image receives `strength` directly. MFLUX-backed
`mlx-diffusion` models receive the equivalent image-influence value through
MFLUX after Tentgent maps the public Diffusers-style strength.

Response shape matches text-to-image jobs:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List transformed files after completion:

```http
GET /v1/images/transforms/job/{job_id}/files
```

Download one transformed file:

```http
GET /v1/images/transforms/job/{job_id}/files/{file_id}
```

Before completion, file routes return HTTP `409` with `result_pending`.
Terminal failures mirror the text-to-image job behavior.

### Image Inpaint Jobs

Canonical masked inpainting uses a workflow job:

```http
POST /v1/images/inpaint/job
Content-Type: multipart/form-data
```

The request uploads one base image and one mask image as bytes, not
client-local paths. Mask semantics are `white = repaint` and `black = keep`.
Tentgent normalizes the mask to binary grayscale before runtime execution.

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | PNG, JPEG, or WebP base image. |
| `mask` | yes | file bytes | PNG, JPEG, or WebP mask image. White pixels repaint; black pixels keep. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt describing the repaint. |
| `negative_prompt` | no | string | Optional negative prompt when the selected backend supports it. |
| `strength` | no | number | Defaults to `1.0`. Must be 0..1. `0.0` preserves the masked area most; `1.0` repaints most. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

Validation happens before model loading where practical:

- `image` and `mask` are both required and must be non-empty.
- Both file parts must be PNG, JPEG, or WebP by content type or file name.
- The Python runtime decodes both files with Pillow and requires matching
  decoded dimensions before resizing both to the requested output size.
- Diffusers inpainting receives `strength` directly.
- MFLUX inpainting requires a Flux Fill-compatible MLX model and maps
  Tentgent strength to the MFLUX image-influence parameter.

Response shape matches other image generation jobs:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List inpainted files after completion:

```http
GET /v1/images/inpaint/job/{job_id}/files
```

Download one inpainted file:

```http
GET /v1/images/inpaint/job/{job_id}/files/{file_id}
```

Before completion, file routes return HTTP `409` with `result_pending`.
Terminal failures mirror the text-to-image job behavior.

### Image Control Jobs

Canonical controlled image generation uses a workflow job:

```http
POST /v1/images/control/job
Content-Type: multipart/form-data
```

The request uploads one control image as bytes, not a client-local path. The
control image is paired with a managed ControlNet-style adapter referenced by
`control_ref`. This is separate from the optional image LoRA `adapter_ref`.

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `control_image` | yes | file bytes | PNG, JPEG, or WebP control image. |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `control_ref` | yes | string | Managed ControlNet-style adapter ref or unique short-ref prefix. |
| `control_kind` | no | string | Defaults to `canny`; M6O supports `canny`. |
| `control_strength` | no | number | Defaults to `1.0`. Must be 0..2. Maps to Diffusers ControlNet conditioning scale. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt for the generated image. |
| `negative_prompt` | no | string | Optional negative prompt when the selected backend supports it. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

M6O expects the uploaded image to already be the control representation for the
selected `control_kind`; the daemon does not auto-run canny/depth/pose
preprocessors. Diffusers ControlNet is the first supported backend path. MLX
diffusion control returns an unsupported-backend error until a stable local
ControlNet-capable runtime is integrated.

For tiny ControlNet smoke fixtures, pass explicit small dimensions such as
`width=64`, `height=64`, and `steps=2`. The default `512x512` and `20` steps are
intended for normal image jobs and may be slow or exceed backend memory limits
on PyTorch MPS.

Response shape matches other image generation jobs:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

List controlled generation files after completion:

```http
GET /v1/images/control/job/{job_id}/files
```

Download one controlled generation file:

```http
GET /v1/images/control/job/{job_id}/files/{file_id}
```

Before completion, file routes return HTTP `409` with `result_pending`.
Terminal failures mirror the text-to-image job behavior.

## Related Guides

[Image generation](./generate.md) · [Adapters](../../adapters.md) · [Jobs and results](../../jobs.md)
