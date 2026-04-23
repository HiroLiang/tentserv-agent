# Model Store

This document defines the Tentgent model-store MVP under `TENTGENT_HOME/models`.

## Purpose

- Keep imported and pulled model assets under Tentgent-managed storage.
- Deduplicate models by content rather than by source name.
- Preserve source indexes for local imports and Hugging Face pulls without making those indexes the canonical ownership path.

## Layout

```text
TENTGENT_HOME/
└── models/
    ├── store/
    │   └── <model_ref>/
    │       ├── model.toml
    │       ├── manifest.json
    │       └── variants/
    │           └── <primary_format>/
    │               ├── variant.toml
    │               └── source/
    ├── by-source/
    │   ├── hf/
    │   │   └── <escaped_repo_id>/
    │   │       └── <resolved_sha>.toml
    │   └── local/
    │       └── <model_ref>.toml
    └── staging/
```

## Canonical Identity

- The canonical model identity is `model_ref`.
- `model_ref` is the SHA-256 of the canonical manifest JSON bytes.
- The manifest JSON records every regular file under the staged import root with:
  - `relative_path`
  - `size_bytes`
  - `sha256`
- Sort manifest entries by normalized relative path before hashing.
- `short_ref` is the first 12 hexadecimal characters of `model_ref`.

## Deduplication Rule

- If `models/store/<model_ref>/` already exists, Tentgent must not copy or download the same content again.
- In that case, Tentgent should update or create the relevant source index entry and reuse the existing canonical store directory.

## Removal Rule

- `tentgent model rm <HASH>` should resolve the model by full hash or unique short-hash prefix.
- Removing a model should delete the canonical store directory under `models/store/<model_ref>/`.
- Removing a model should also delete related source indexes in both `models/by-source/local/` and `models/by-source/hf/`.
- Empty Hugging Face source-index directories should be cleaned up after removal.

## Metadata

`model.toml` should record:

- `model_ref`
- `short_ref`
- `source_kind`
- `source_repo`
- `source_revision`
- `source_path`
- `primary_format`
- `detected_formats`
- `file_count`
- `total_bytes`
- `imported_at`

`variant.toml` should record:

- `format`
- `status = "imported"`
- `import_method = "add" | "pull"`
- `relative_source_path = "source"`

## Format Detection

- Detect `gguf` when any file ends with `.gguf`.
- Detect `safetensors` when any file ends with `.safetensors` or a filename equals `model.safetensors.index.json`.
- Detect `mlx` only for Hugging Face repositories under `mlx-community/*` in this MVP.
- Mixed-format sources are allowed, but Tentgent stores one primary format per canonical model in this MVP.

Primary format selection order:

1. `mlx` for `mlx-community/*`
2. `safetensors` when detected
3. `gguf` when detected

## Hugging Face Pull Contract

- `tentgent model pull` should resolve the requested repo to an exact commit SHA before download.
- The Rust core invokes the `tentgent-hf-snapshot` entry point from the `python/tentgent-daemon` subproject.
- The helper implementation lives in `python/tentgent-daemon/src/tentgent_daemon/tools/hf_snapshot.py`.
- The helper should run from the Python subproject root through `uv run tentgent-hf-snapshot ...` so the Python subproject remains the single source of truth for package resolution.
- Callers outside the subproject may still use `uv run --project python/tentgent-daemon ...` for manual testing.
- The helper should use `huggingface_hub` `model_info()` plus `snapshot_download()`.
- The effective `HF_TOKEN` should be passed through when available.
- The helper should materialize a full snapshot into Tentgent staging and return JSON containing:
  - `repo_id`
  - `resolved_revision`
  - `local_dir`
