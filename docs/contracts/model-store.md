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
- Removing a model must be blocked when any stored Tentgent server spec still references that model.
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
- `model_capabilities`
- `model_capability_source`
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
- Format detection does not guarantee the current machine can run the model. Backend capability rules are defined in [platform-backends.md](./platform-backends.md).

Primary format selection order:

1. `mlx` for `mlx-community/*`
2. `safetensors` when detected
3. `gguf` when detected

## Model Capability Metadata

`model_capabilities` describes what endpoint families a model can serve. It is
separate from file format and local backend readiness.

Initial capability values:

- `chat`
- `embedding`
- `rerank`

Existing metadata without `model_capabilities` should be read as `["chat"]`.
New imports default to `["chat"]` when the user does not provide an explicit
capability. Local import and Hugging Face pull accept one explicit capability
value for this milestone: `chat`, `embedding`, or `rerank`. Explicit input
stores exactly that one value and records `model_capability_source =
"explicit-user"`.

If content deduplication finds an existing `model_ref`, omitted capability
input preserves the stored metadata. Explicit capability input updates the
existing stored model metadata before returning the deduplicated outcome. This
metadata update does not copy content and does not change `model_ref`.

Hugging Face metadata auto-classification is not implemented yet. A model may
list multiple capabilities later when source metadata, user edits, or another
explicit update path proves that it supports more than one serving shape.

`model_capability_source` records why the current capability set was chosen:

- `default-chat`: backward-compatible default for old metadata or imports
  without explicit evidence.
- `explicit-user`: import or pull input explicitly declared capabilities.
- `huggingface-metadata`: source metadata such as pipeline tags or
  architectures provided enough evidence.
- `manual-update`: a later local metadata mutation changed the capability set.

Changing capability metadata does not change `model_ref`; canonical identity is
still content-derived from the manifest. A later metadata update may add
`embedding` or `rerank` to an imported model when the user or source metadata
can justify it.

Capability metadata is descriptive only in this milestone. It does not imply
that `/v1/embeddings`, `/v1/rerank`, embedding runtime ports, or rerank runtime
ports exist.

## Hugging Face Pull Contract

- `tentgent model pull` should resolve the requested repo to an exact commit SHA before download.
- The Rust core invokes the `tentgent-hf-snapshot` entry point through the shared Python runtime asset resolver.
- The helper implementation lives in `python/tentgent-daemon/src/tentgent_daemon/tools/hf_snapshot.py`.
- In development, the resolver falls back to `python/tentgent-daemon`.
- In installed builds, the resolver should find `share/tentgent/python` relative to the `tentgent` binary.
- The helper should prefer an existing `tentgent-hf-snapshot` entry point in the resolved Python environment.
- When the entry point is missing, Rust may fall back to `uv --no-config run --project <resolved-python-project> tentgent-hf-snapshot ...` with `UV_PROJECT_ENVIRONMENT` set to the resolved Python environment so the Python subproject remains the single source of truth for package resolution and `uv` does not inspect the repository-root `pyproject.toml`.
- The helper should use `huggingface_hub` `model_info()` plus `snapshot_download()`.
- The effective `HF_TOKEN` should be passed through when available.
- Rust owns CLI progress rendering for `model pull`.
- The Python helper should keep native `huggingface_hub` progress bars disabled and emit JSON Lines progress events when called with `--progress-json`.
- The Rust core parses those JSON Lines events and the CLI renders one terminal progress bar, avoiding nested tqdm output.
- The helper should materialize a full snapshot into Tentgent staging and return JSON containing:
  - `repo_id`
  - `resolved_revision`
  - `local_dir`
