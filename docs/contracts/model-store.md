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
- `mlx_runtime_family` when `primary_format = "mlx"` and Tentgent can infer a
  single MLX runtime family from model capability metadata
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

`model_capabilities` describes the intended endpoint family for a model. It is
separate from file format, local backend readiness, and whether a runtime path
has been implemented yet.

Initial capability values:

- `chat`
- `embedding`
- `rerank`
- `audio-transcription`
- `audio-speech`
- `vision-chat`
- `video-understanding`
- `image-generation`

Existing metadata without `model_capabilities` should be read as `["chat"]`.
New imports default to `["chat"]` when neither explicit input nor confident
Hugging Face metadata evidence identifies a capability. Local import and
Hugging Face pull accept one explicit capability value. Explicit input stores
exactly that one value and records `model_capability_source =
"explicit-user"`.

If content deduplication finds an existing `model_ref`, omitted capability
input preserves the stored metadata. Explicit capability input updates the
existing stored model metadata before returning the deduplicated outcome. This
metadata update does not copy content and does not change `model_ref`.

Hugging Face pull uses compact registry and snapshot metadata as best-effort
evidence. `feature-extraction`, `sentence-similarity`,
`sentence-transformers`, or `sentence_bert_config.json` can classify
`embedding`; `text-ranking`, `reranker`, `rerank`, `cross-encoder`, or
sequence-classification paired with ranking metadata can classify `rerank`;
`text-generation`, `conversational`, or a tokenizer `chat_template` can
classify `chat`. Conflicting or weak evidence must not be guessed.

Media capabilities are explicit-only in M6A: Hugging Face audio, image, video,
or vision pipeline tags must not auto-classify a model as
`audio-transcription`, `audio-speech`, `vision-chat`, `video-understanding`, or
`image-generation`.

Capability assignment precedence is:

1. explicit request input (`explicit-user`)
2. protected stored user correction (`manual-update`)
3. confident Hugging Face metadata (`huggingface-metadata`)
4. deduplicated existing metadata
5. default fallback (`default-chat`)

Automatic Hugging Face detection may update deduplicated metadata only when the
existing source is `default-chat` or `huggingface-metadata`. It must not
overwrite `explicit-user` or `manual-update` unless the current request
explicitly provides a capability.

A model may list multiple capabilities when source metadata, user edits, or
another explicit update path proves that it supports more than one serving
shape. Manual capability updates may replace the whole capability set or apply
add/remove mutations. Stored capability values are de-duplicated and written in
canonical model capability order. Mutations that would leave a model with no
capabilities are invalid.

## MLX Runtime Family Metadata

`primary_format = "mlx"` records the storage layout family. It does not by
itself mean the model should be loaded through `mlx-lm`.

`mlx_runtime_family` is optional and records the intended MLX runtime package
family when Tentgent can infer exactly one family from the model capabilities:

- `mlx-lm` for `chat`
- `mlx-vlm` for `vision-chat`; direct Python model-runtime
  `video-understanding` supports only allow-listed MLX VLM video model types
- `mlx-audio` for `audio-transcription` and `audio-speech`
- `mlx-diffusion` for `image-generation`

Existing metadata without `mlx_runtime_family` remains valid. For backward
compatibility, `primary_format = "mlx"` plus missing family is treated as the
legacy chat path when the model advertises `chat`.

Tentgent should leave `mlx_runtime_family` unset when the model is not MLX,
when the capability has no approved MLX runtime family, or when multiple
capabilities would imply conflicting MLX runtime families. Updating explicit or
manual model capability metadata recalculates this field without changing
`model_ref`.

`model_capability_source` records why the current capability set was chosen:

- `default-chat`: backward-compatible default for old metadata or imports
  without explicit evidence.
- `explicit-user`: import or pull input explicitly declared capabilities.
- `huggingface-metadata`: source metadata such as pipeline tags or
  architectures provided enough evidence.
- `manual-update`: a later local metadata mutation changed the capability set.

Changing capability metadata does not change `model_ref`; canonical identity
is still content-derived from the manifest. A later metadata update may change
the stored endpoint-family intent when the user or source metadata can justify
it.

Capability metadata is endpoint-gating metadata. Chat endpoints accept only
models advertising `chat`; the embedding endpoint accepts only models
advertising `embedding`; the rerank endpoint accepts only models advertising
`rerank`. Media endpoint families require matching media capabilities before
runtime dispatch, including `audio-transcription`, `audio-speech`,
`vision-chat`, `video-understanding`, and `image-generation`. Direct local
server routes for media families remain separate contracts.

## Capability Proof Metadata

Capability proofs record the latest observed result for a
`model_ref + capability` pair. They are separate from declared
`model_capabilities`: declaration says the model should be routed to an
endpoint family, while proof records whether a local probe or runtime event has
confirmed or failed that path.

Proof records are raw evidence. The effective model support status vocabulary
and precedence rules are defined in
[model-support-status.md](./model-support-status.md). A `verified` proof can
contribute to effective `verified`, a `failed` proof can contribute to
effective `failed`, and either proof can become effectively `stale` when its
model, capability, backend, runtime, platform, or resolver assumptions no
longer match the current tuple.

The proof and support hint schema is defined in
[model-support-proof-schema.md](./model-support-proof-schema.md). The schema
separates local proof from built-in or shared support hints. Local proof records
may explain `verified` or `failed`; support hints may explain `supported` or
`unsupported`.

Tuple-aware proofs live under the canonical model directory:

```text
models/store/<model_ref>/support-proofs/<capability>/<proof_key>.toml
```

The legacy latest-proof path remains readable and writable for compatibility:

```text
models/store/<model_ref>/capability-proofs/<capability>.toml
```

Current proof records store a legacy subset:

- `model_ref`
- `capability`
- `status = "verified" | "failed"`
- `source = "manual-probe" | "server-start" | "endpoint-smoke" |
  "runtime-execution"`
- `primary_format`
- optional `mlx_runtime_family`
- `backend`
- optional `runtime_version`
- optional `server_ref`
- `checked_at`
- optional `error`

The `v0.7.0` support-status implementation should expand records toward the
versioned schema without breaking reads of the legacy subset.

The current manual probe is metadata-level: it verifies that the stored model
advertises the requested capability and records the inferred backend label. A
server start writes a `server-start` proof for local model-bound servers after
the launch path succeeds or fails. Resolved local runtime attempts write
`runtime-execution` proofs after execution succeeds or fails. Endpoint-level
smoke proofs can later reuse the same file shape with
`source = "endpoint-smoke"`.

## Hugging Face Pull Contract

- `tentgent model pull` should resolve the requested repo to an exact commit SHA before download.
- The Rust core invokes the `tentgent-hf-snapshot` entry point through the shared Python runtime asset resolver.
- The helper implementation lives in `python/tentgent-model-runtime/src/tentgent/runtime/tools/hf_snapshot.py`.
- In development, the resolver falls back to `python/tentgent-model-runtime`.
- In installed builds, the resolver should find `share/tentgent/python/tentgent-model-runtime` relative to the `tentgent` binary.
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
