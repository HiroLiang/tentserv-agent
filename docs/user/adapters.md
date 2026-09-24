# Adapters

Use a managed adapter with the compatible base model. A successful [LoRA run](./training-lora.md) imports its adapter automatically; external adapters can be imported or pulled.

## Examples And Common Operations

Import or pull adapters:

```bash
tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
tentgent adapter ls
tentgent adapter inspect <adapter-ref>
tentgent adapter bind <adapter-ref> --base-model-ref <model-ref>
```

Adapter compatibility establishes that it can be selected for the base model;
the resulting answer style depends on its training data and weights.
Image-generation LoRA adapters should include `--target-capability
image-generation`, a backend such as `diffusers` or `mlx-diffusion`, and
`--weight-file` when the source has more than one `.safetensors` file. The
daemon `/v1/adapters/import`, `/v1/adapters/pull`, and their `/jobs` variants
accept the same image LoRA metadata as JSON fields.

ControlNet-style image control adapters should be imported or pulled as a
separate control adapter, not as an image LoRA:

```bash
tentgent adapter pull <hf-controlnet-repo> \
  --base-model-ref <image-generation-model-ref> \
  --target-capability image-generation \
  --adapter-type controlnet \
  --adapter-format diffusers-controlnet \
  --backend-support diffusers \
  --control-kind canny
```

Binding records compatibility with a managed base model; it does not merge weights or make every request use the adapter. Select it explicitly for inference:

```bash
tentgent chat <model-ref> --adapter-ref <adapter-ref> --message "user:Hello"
tentgent adapter rm <adapter-ref>
```

`rm` deletes the managed adapter and its indexes. Referencing resources may block removal. The original imported directory is retained.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `adapter add/pull` | `-b, --base-model-ref <MODEL_REF>` | Local base model reference this adapter was trained for |
| `adapter add/pull` | `--target-capability <CAPABILITY>` | Target model capability this adapter is intended for, such as image-generation |
| `adapter add/pull` | `--adapter-type <TYPE>` | Adapter type override, such as lora or controlnet |
| `adapter add/pull` | `--adapter-format <FORMAT>` | Adapter format override, such as diffusers-lora, diffusers-controlnet, or mlx-diffusion-lora |
| `adapter add/pull` | `--backend-support <BACKEND>` | Runtime backend support override. May be repeated |
| `adapter add/pull` | `--control-kind <KIND>` | Control kind for ControlNet-style adapters, such as canny |
| `adapter add/pull` | `--weight-file <RELATIVE_PATH>` | Relative path to the LoRA weight file inside the adapter source |
| `adapter add/pull` | `--trigger-word <TEXT>` | Trigger word hint for image LoRA prompts. May be repeated |
| `adapter add/pull` | `--recommended-scale <FLOAT>` | Recommended LoRA scale to store as adapter metadata |
| `adapter pull` | `-r, --revision <REV>` | Hugging Face revision to resolve before downloading |
| `adapter bind` | `-b, --base-model-ref <MODEL_REF>` | Local managed base model reference this adapter should target |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/adapters` | None. |
| `GET` | `/v1/adapters/{reference}` | None. |
| `DELETE` | `/v1/adapters/{reference}` | None. |
| `POST` | `/v1/adapters/import` | Absolute daemon-host `path`, plus optional metadata below. |
| `POST` | `/v1/adapters/pull` | `repo_id`, optional `revision`, plus metadata below. |
| `POST` | `/v1/adapters/import/jobs` | Same as `/v1/adapters/import`, returns a job. |
| `POST` | `/v1/adapters/pull/jobs` | Same as `/v1/adapters/pull`, returns a job. |
| `POST` | `/v1/adapters/{reference}/bind` | `{"base_model_ref":"<base-model-ref>"}` |

### Import And Pull Fields

Import requires `path`; pull requires `repo_id` and accepts an optional
`revision`. Shared metadata fields map from CLI options as follows:

| CLI option | HTTP field | Type |
| --- | --- | --- |
| `--base-model-ref` | `base_model_ref` | Managed model ref string. |
| `--target-capability` | `target_capability` | Capability string. |
| `--adapter-type`, `--adapter-format` | `adapter_type`, `adapter_format` | Strings identifying type and format. |
| `--backend-support` (repeatable) | `backend_support` | Array of backend strings. |
| `--control-kind` | `control_kind` | Control representation string, such as `canny`. |
| `--weight-file` | `weight_file` | Relative source weight filename. |
| `--trigger-word` (repeatable) | `trigger_words` | Array of prompt hint strings. |
| `--recommended-scale` | `recommended_scale` | Optional numeric LoRA scale hint. |

Example image LoRA import body:

```json
{
  "path": "/absolute/path/on/daemon-host/image-lora",
  "base_model_ref": "<image-model-ref>",
  "target_capability": "image-generation",
  "adapter_type": "lora",
  "adapter_format": "diffusers-lora",
  "backend_support": ["diffusers"],
  "weight_file": "pytorch_lora_weights.safetensors",
  "recommended_scale": 0.8
}
```

Adapter import and pull metadata fields are optional. For image-generation LoRA
adapters, set `target_capability` to `image-generation`, use `adapter_format`
`diffusers-lora` or `mlx-diffusion-lora`, and include the corresponding
`backend_support` value `diffusers` or `mlx-diffusion`. `weight_file` is
required when the adapter source contains multiple candidate `.safetensors`
files. The `/jobs` variants accept the same metadata fields and return a daemon
job immediately.

For ControlNet-style image control adapters, set `target_capability` to
`image-generation`, `adapter_type` to `controlnet`, `adapter_format` to
`diffusers-controlnet`, `backend_support` to `["diffusers"]`, and
`control_kind` to a supported kind such as `canny`.

### Bind Through HTTP

```bash
curl -sS -X POST 'http://127.0.0.1:8790/v1/adapters/<adapter-ref>/bind' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"base_model_ref":"<model-ref>"}'
```

List returns `adapters`; inspect returns `adapter`. Import, pull, and bind
return `adapter` and `mutation`; use `adapter.adapter_ref` in inference.
Import paths are absolute daemon-host paths. `/jobs` variants return a
[job](./jobs.md); guarded operations may return `409` with blockers.
Binding validates available base-model hints and can reject an incompatible base.

## Related Guides

[LoRA training](./training-lora.md) · [Chat](./inference/chat.md) · [Image generation](./inference/images/generate.md)
