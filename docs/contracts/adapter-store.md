# Adapter Store

This document defines the Tentgent adapter-store draft under `TENTGENT_HOME/adapters`.

## Purpose

- Keep LoRA and other adapter assets under Tentgent-managed storage.
- Treat adapters as first-class resources instead of nesting them under one model directory.
- Preserve compatibility metadata so a server can decide whether an adapter may be used with its base model.
- Keep the layout compatible with local imports, Hugging Face pulls, and future training outputs.

## Layout

```text
TENTGENT_HOME/
└── adapters/
    ├── store/
    │   └── <adapter_ref>/
    │       ├── adapter.toml
    │       ├── manifest.json
    │       └── source/
    ├── by-base/
    │   └── <base_model_ref>/
    │       └── <adapter_ref>.toml
    ├── by-source/
    │   ├── hf/
    │   │   └── <escaped_repo_id>/
    │   │       └── <resolved_sha>.toml
    │   ├── local/
    │   │   └── <adapter_ref>.toml
    │   └── train-run/
    │       └── <run_ref>.toml
    └── staging/
```

## Canonical Identity

- The canonical adapter identity is `adapter_ref`.
- `adapter_ref` should be content-derived from a canonical manifest, following the same spirit as `model_ref`.
- The manifest should record every regular file under the staged adapter root with:
  - `relative_path`
  - `size_bytes`
  - `sha256`
- Sort manifest entries by normalized relative path before hashing.
- `short_ref` is the first 12 hexadecimal characters of `adapter_ref`.

## Adapter Metadata

`adapter.toml` should record:

- `adapter_ref`
- `short_ref`
- `adapter_format`
- `adapter_type`
- `target_capability`
- `base_model_ref`
- `base_model_source_repo`
- `base_model_source_revision`
- `model_family`
- `backend_support`
- `control_kind`
- `weight_file`
- `trigger_words`
- `recommended_scale`
- `source_kind`
- `source_repo`
- `source_revision`
- `source_path`
- `training_dataset_ref`
- `training_run_ref`
- `training_config_ref`
- `file_count`
- `total_bytes`
- `imported_at`

Notes:

- `base_model_ref` should be preferred when the exact local base model is known.
- `base_model_source_repo` and `base_model_source_revision` help match adapters pulled from Hugging Face when the local `model_ref` is not known yet.
- `model_family` is a weaker compatibility hint and should not replace exact compatibility metadata.
- `target_capability` describes the model capability the adapter should be used
  with. Existing chat adapters may omit it and are treated as legacy chat
  adapters by compatibility checks.
- `backend_support` should describe intended runtime support, such as
  `transformers-peft`, `mlx`, `diffusers`, `mlx-diffusion`, or `llama-cpp`.
- `control_kind` is used by ControlNet-style image control adapters. M6O
  supports `canny`.
- `weight_file` is used by image LoRA adapters when the managed `source/`
  directory contains one selected `.safetensors` file.
- `trigger_words` are prompt hints only. Tentgent records and displays them but
  does not inject them into prompts.
- `recommended_scale` is a metadata hint. Requests still choose the actual
  `lora_scale`.

## Compatibility Rule

Tentgent should not treat an adapter as universally compatible.

`tentgent adapter add <PATH> --base-model-ref <MODEL_REF>` may bind a local adapter import to one managed base model during import.

`tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>` should bind an already imported adapter to one managed base model after the base model becomes available.

Imports without a local base model are valid. They should preserve any source-level base-model hints from `adapter_config.json`, but they should not prompt for a model or pull one from the network by default.

Before a server uses an adapter, it should check:

- the adapter exists in `adapters/store/<adapter_ref>/`
- the server base model matches `base_model_ref` when that field is present
- otherwise, the server can fall back to source repo and revision compatibility checks
- the adapter target capability matches the request capability when
  `target_capability` is present
- the adapter backend support includes the server backend
- the adapter type matches the workflow: `lora` for LoRA selection and
  `controlnet` for image control selection
- ControlNet-style image control requests must match the adapter
  `control_kind`
- the server policy allows that adapter

The first implementation should be conservative. If compatibility cannot be proven, reject the request with a clear error.

When `adapter_config.json` contains base-model hints, Tentgent should compare them with the selected local base model:

- PEFT adapters commonly use `base_model_name_or_path` and optional `revision`.
- MLX training output may use `model`.
- If the adapter base model hint and the local model source repo disagree, reject the binding.
- If both sides provide revisions and they disagree, reject the binding.

## Source Shape

For PEFT-style LoRA adapters, `source/` commonly contains:

- `adapter_model.safetensors`
- `adapter_config.json`
- `README.md`

Other backend-specific adapter formats may use different filenames. Tentgent should keep the raw adapter source intact and store Tentgent metadata beside it.

For Diffusers image LoRA adapters, `source/` commonly contains:

- `pytorch_lora_weights.safetensors`
- `README.md`

For MFLUX-backed MLX diffusion image LoRA adapters, `source/` may contain one
Flux-compatible `.safetensors` file. If an image LoRA source contains multiple
`.safetensors` files, import or pull should specify `--weight-file` so runtime
execution uses one deterministic local managed file.

For Diffusers ControlNet-style image control adapters, `source/` is the raw
ControlNet repository snapshot and should be imported or pulled with:

- `adapter_type = "controlnet"`
- `adapter_format = "diffusers-controlnet"`
- `target_capability = "image-generation"`
- `backend_support = ["diffusers"]`
- `control_kind = "canny"` for the first supported workflow

ControlNet adapters are not LoRA adapters. Runtime execution combines one base
image-generation model, one optional image LoRA adapter, one ControlNet adapter,
and one uploaded control image.

## Indexes

- `by-base/<base_model_ref>/<adapter_ref>.toml`
  Fast lookup for adapters known to target one local base model.
- `by-source/hf/<escaped_repo_id>/<resolved_sha>.toml`
  Lookup for remote adapter imports by Hugging Face repo and resolved revision.
- `by-source/local/<adapter_ref>.toml`
  Lookup for local adapter imports.
- `by-source/train-run/<run_ref>.toml`
  Lookup from one training run to its produced adapter.

Indexes are lookup aids only. Canonical ownership lives under `store/<adapter_ref>/`.

## List Display

`adapter ls` should keep rows compact:

- Hugging Face sources show `<repo>@<short_revision>`.
- Training-run sources show `run:<short_run_ref>`.
- Local sources show the original path only when short, otherwise the final path component.

Full provenance remains available through `adapter inspect <ADAPTER_REF>`.

## Hugging Face Pull

`tentgent adapter pull <HF_REPO> [--revision <REV>] [--base-model-ref <MODEL_REF>]` should:

- resolve the requested repo to an exact commit SHA through the shared `tentgent-hf-snapshot` helper
- prefer an existing `tentgent-hf-snapshot` entry point in the resolved Python environment
- fall back to `uv --no-config run --project <resolved-python-project> ...` with `UV_PROJECT_ENVIRONMENT` set to the resolved Python environment only when the entry point is missing
- use the shared Python runtime asset resolver so development falls back to `python/tentgent-model-runtime` and installed builds use `share/tentgent/python`
- download the full snapshot into adapter staging
- build the normal adapter manifest and content-derived `adapter_ref`
- write `source_kind = "huggingface"`, `source_repo`, and `source_revision`
- create `by-source/hf/<escaped_repo_id>/<resolved_sha>.toml`
- optionally validate and bind to `--base-model-ref` using the same rules as local `adapter add`

The first pull implementation intentionally imports the full adapter repository snapshot. File filtering can be added later if large training artifacts become a practical problem.

## Removal Rule

- `tentgent adapter rm <ADAPTER_REF>` should resolve by full hash or unique short-hash prefix.
- Removing an adapter should be blocked when any stored server spec explicitly allows, preloads, or defaults to that adapter. Current removal protection recognizes future server-spec fields such as `adapter_ref`, `default_adapter_ref`, `allowed_adapters`, and `adapter_refs`.
- Removing an adapter should delete the canonical store directory under `adapters/store/<adapter_ref>/`.
- Removing an adapter should also delete related `by-base` and `by-source` index entries.

## First Implementation Boundary

The first adapter-store implementation should support:

- local adapter import
- Hugging Face adapter pull
- training-run adapter import
- metadata inspection
- explicit local base-model binding
- compatibility checks for `safetensors + PEFT`
- image-generation LoRA metadata and one selected local `.safetensors` weight
  file for Diffusers or `mlx-diffusion` generation

It should not require:

- multi-server coordination
- remote adapter hosting by Tentgent
- dynamic download during a live chat request
- feature parity across `mlx` and `llama-cpp`
