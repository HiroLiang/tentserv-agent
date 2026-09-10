# Image Generation

Generate one image from text using an `image-generation` model. Prepare the [media runtime](../../runtime.md#media-runtime-dependencies); model and adapter requirements are covered by the [image workflow index](./README.md).

## Examples And Common Operations

Run foreground text-to-image generation without starting the daemon:

```bash
tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --prompt "A small ceramic teapot on a wooden table" \
  --output image.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

The command always writes to `--output` and fails before running if that file
already exists. Supported output formats are `png` and `jpg`. Width and height
must be between 64 and 1024 pixels and divisible by 8. Steps must be 1 through
100, and guidance scale must be 0 through 30. Diffusers image generation
defaults to the first available supported device. MLX image-generation models
with `mlx_runtime_family = mlx-diffusion` run through MFLUX on Apple Silicon
macOS after the `local-model` runtime profile is bootstrapped. You can force
one Diffusers command to CPU when debugging Apple MPS or CUDA issues:

```bash
TENTGENT_IMAGE_GENERATION_DEVICE=cpu tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --prompt "A smiling face avatar" \
  --output avatar.png
```

Use one managed image LoRA adapter by importing or pulling it first, then pass
the adapter reference to the same image command:

```bash
tentgent adapter pull <hf-image-lora-repo> \
  --base-model-ref <image-generation-model-ref> \
  --target-capability image-generation \
  --adapter-format diffusers-lora \
  --backend-support diffusers \
  --weight-file pytorch_lora_weights.safetensors \
  --trigger-word "<optional-trigger>"

tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --adapter-ref <image-lora-adapter-ref> \
  --lora-scale 0.8 \
  --prompt "A smiling face avatar, <optional-trigger>" \
  --output avatar.png
```

For MFLUX-backed `mlx-diffusion` models, use
`--adapter-format mlx-diffusion-lora --backend-support mlx-diffusion` and a
Flux-compatible local `.safetensors` weight file. Trigger words are hints only;
Tentgent does not rewrite prompts.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `image generate` | `-m, --model-ref <MODEL_REF>` | Stored Tentgent image-generation model reference to run |
| `image generate` | `-p, --prompt <TEXT>` | Prompt for the generated image |
| `image generate` | `--negative-prompt <TEXT>` | Optional negative prompt |
| `image generate` | `--adapter-ref <ADAPTER_REF>` | Optional managed image LoRA adapter reference |
| `image generate` | `--lora-scale <FLOAT>` | Optional LoRA scale. Requires --adapter-ref |
| `image generate` | `-o, --output <OUTPUT_PATH>` | Local output path. Existing files are never overwritten |
| `image generate` | `--format <FORMAT>` | Output image format intent: png or jpg [default: png] |
| `image generate` | `--width <PX>` | Output image width in pixels. Must be 64..1024 and divisible by 8 [default: 512] |
| `image generate` | `--height <PX>` | Output image height in pixels. Must be 64..1024 and divisible by 8 [default: 512] |
| `image generate` | `--steps <N>` | Diffusion inference steps. Must be 1..100 [default: 20] |
| `image generate` | `--guidance-scale <FLOAT>` | Classifier-free guidance scale. Must be 0..30 [default: 7.5] |
| `image generate` | `--seed <N>` | Optional deterministic seed |
| `image generate` | `-H, --home <HOME>` | Optional Tentgent runtime home override |

## HTTP API

Start the [daemon](../../daemon.md) and follow the [HTTP authentication and error rules](../../api.md).

Canonical text-to-image generation uses a workflow job:

```http
POST /v1/images/generations/job
Content-Type: application/json
```

Request body:

```json
{
  "model_ref": "<image-generation-model-ref>",
  "adapter_ref": "<optional-image-lora-adapter-ref>",
  "lora_scale": 0.8,
  "prompt": "A small ceramic teapot on a wooden table",
  "negative_prompt": "optional negative prompt",
  "output_format": "png",
  "output_filename": "teapot.png",
  "width": 512,
  "height": 512,
  "steps": 20,
  "guidance_scale": 7.5,
  "seed": 42
}
```

Fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `adapter_ref` | no | string | Optional managed image LoRA adapter ref or unique short-ref prefix. |
| `lora_scale` | no | number | Optional LoRA scale. Defaults to `1.0` when `adapter_ref` is present; must be 0..4. |
| `prompt` | yes | string | Text prompt for image generation. |
| `negative_prompt` | no | string | Optional negative prompt. |
| `output_format` | no | string | `png` or `jpg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `image.<format>`. |
| `width` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `height` | no | integer | Defaults to 512. Must be 64..1024 and divisible by 8. |
| `steps` | no | integer | Defaults to 20. Must be 1..100. |
| `guidance_scale` | no | number | Defaults to 7.5. Must be 0..30. |
| `seed` | no | integer | Optional deterministic seed. |

The daemon uses the same image-generation runtime selection as the CLI:
Diffusers models use the Diffusers backend, and MLX `mlx-diffusion` models use
MFLUX on Apple Silicon macOS. Set `TENTGENT_IMAGE_GENERATION_DEVICE=cpu`,
`mps`, or `cuda` before daemon startup to force a Diffusers device for
image-generation jobs. Set
`TENTGENT_IMAGE_GENERATION_TORCH_DTYPE=float32` or `float16` only for
model/runtime compatibility debugging.
When `adapter_ref` is present, the daemon validates the adapter against the
selected image-generation model and backend before runtime execution.

Response:

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

List generated files after completion:

```http
GET /v1/images/generations/job/{job_id}/files
```

Example response:

```json
{
  "files": [
    {
      "file_id": "teapot.png",
      "filename": "teapot.png",
      "media_type": "image/png",
      "format": "png",
      "total_bytes": 12345
    }
  ]
}
```

Download one generated file:

```http
GET /v1/images/generations/job/{job_id}/files/{file_id}
```

File download returns the image bytes with `Content-Type`,
`Content-Disposition`, `x-tentgent-job-id`, and `x-tentgent-file-id` headers.
Before completion, file routes return HTTP `409` with `result_pending`.
Failed, interrupted, or canceled jobs return clear terminal conflict errors.

## Related Guides

[Image editing](./edit.md) · [Adapters](../../adapters.md) · [Jobs and results](../../jobs.md)
