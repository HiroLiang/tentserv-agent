# M2 Model Capability Detection And Correction

## Summary

M2 makes model capability metadata less manual without pretending detection is
perfect. Hugging Face pull should classify common embedding and rerank models
from explicit registry evidence when the user does not pass `--capability`.
Users should also have a direct correction path for stored model metadata.

This is still a metadata-boundary milestone. It does not add embedding/rerank
runtime ports, daemon endpoints, direct server endpoints, or backend packages.

## Goals

- Detect `embedding` and `rerank` for common Hugging Face models when metadata
  evidence is specific enough.
- Keep explicit user input as the highest-priority authority.
- Preserve user-corrected metadata from later automatic detection.
- Add a manual capability correction command and REST route.
- Warn users when import or pull falls back to `default-chat` because evidence
  was absent or ambiguous.

## Non-Goals

- No `/v1/embeddings` or `/v1/rerank`.
- No local embedding/rerank backend execution.
- No audio capability naming or detection.
- No broad local import auto-detection from filenames alone.
- No claim that every Hugging Face architecture can be classified.

## Precedence Rules

Capability assignment uses this priority order:

1. Explicit request input: `--capability` or JSON `capability`; source
   `explicit-user`.
2. Manual metadata update: stored metadata source `manual-update`.
3. Confident Hugging Face metadata detection; source `huggingface-metadata`.
4. Existing stored metadata on deduplicated content.
5. Default fallback: `["chat"]`; source `default-chat`.

Automatic Hugging Face detection may update deduplicated metadata only when the
existing source is `default-chat` or `huggingface-metadata`. It must not
overwrite `explicit-user` or `manual-update` unless the current request includes
an explicit `capability`.

## Detection Inputs

Extend the Hugging Face snapshot helper output with compact metadata gathered
from `HfApi.model_info()` and files already present in the downloaded snapshot:

- `pipeline_tag`
- repo tags
- library name when available
- config architectures from `config.json`
- tokenizer chat template presence from `tokenizer_config.json`
- existence of known auxiliary files such as `sentence_bert_config.json`

Do not persist raw Hugging Face metadata in `model.toml` for M2. Persist only
the chosen `model_capabilities` and `model_capability_source`.

## Detection Rules

Prefer specific positive evidence over broad format or filename guesses.

Classify as `embedding` when any strong signal is present and no rerank signal
conflicts:

- `pipeline_tag` is `feature-extraction` or `sentence-similarity`.
- tags or library metadata identify `sentence-transformers`.
- the snapshot contains `sentence_bert_config.json`.

Classify as `rerank` when any strong signal is present:

- `pipeline_tag` is `text-ranking`.
- tags include `reranker`, `rerank`, or `cross-encoder`.
- `config.json` indicates a sequence-classification architecture and registry
  tags identify ranking or reranking intent.

Classify as `chat` from Hugging Face metadata only when evidence is specific:

- `pipeline_tag` is `text-generation` or `conversational`.
- tokenizer config contains a `chat_template`.

When signals conflict or only weak hints are available, do not detect. Preserve
existing metadata on deduplication, or fall back to `default-chat` for new
content.

## Kernel Changes

- Add a small capability classifier module under the model feature, separate
  from format detection.
- Introduce a domain value such as `ModelCapabilityAssignment` containing:
  - `capabilities`
  - `source`
  - optional human-readable `reason`
  - optional warning when fallback happened
- Change the shared import finalizer to accept a resolved capability assignment
  instead of only `Option<ModelCapability>`.
- Resolve the assignment before finalization:
  - local import: explicit input or default fallback
  - HF pull: explicit input, otherwise classifier output, otherwise fallback
- Add a `ModelCapabilityUpdateUseCase` that resolves a model ref/prefix and
  rewrites only `model_capabilities` and `model_capability_source =
  "manual-update"`.

## CLI Changes

Add a correction command:

```bash
tentgent model set-capability <model-ref> <chat|embedding|rerank>
```

The command should print the same metadata detail rows used by import, pull,
and inspect.

For import and pull, print a clear warning when the resulting source is
`default-chat`, for example:

```text
warning: capability defaulted to chat; pass --capability embedding or
--capability rerank if this model serves another endpoint family.
```

## Daemon REST Changes

Add synchronous metadata correction:

```http
PATCH /v1/models/{model_ref}
Content-Type: application/json

{ "capability": "embedding" }
```

Response shape:

```json
{
  "model": {
    "...": "same shape as GET /v1/models/{model_ref}"
  },
  "mutation": {
    "kind": "update_capability"
  }
}
```

Request validation:

- invalid capability returns `400 bad_request`
- missing model returns `404 not_found`
- ambiguous ref returns `409 ambiguous_ref`
- unknown request fields are rejected

For import/pull mutation responses, add an additive warning field only when the
capability source remains `default-chat`:

```json
{
  "warnings": [
    "capability defaulted to chat; provide capability to classify embedding or rerank models"
  ]
}
```

Background job records should place the same message in `warning_summary` when
the completed import or pull remains `default-chat`.

## Docs

- Update `docs/contracts/model-store.md` with M2 detection precedence and
  preservation rules.
- Update `docs/contracts/http-daemon.md` with the PATCH route and warning field.
- Update `docs/user/commands.md` with the correction command and examples.
- Keep docs explicit that capability metadata still does not imply runtime
  availability.

## Test Plan

Kernel tests:

- HF metadata classifies common embedding evidence as `["embedding"]` /
  `huggingface-metadata`.
- HF metadata classifies common rerank evidence as `["rerank"]` /
  `huggingface-metadata`.
- Ambiguous HF metadata falls back to `["chat"]` / `default-chat` with warning.
- Explicit capability overrides detected metadata.
- Deduplicated HF detection updates only `default-chat` or
  `huggingface-metadata` metadata.
- Deduplicated HF detection does not overwrite `explicit-user` or
  `manual-update`.
- Manual update rewrites capability metadata without changing `model_ref`.

CLI tests:

- `tentgent model set-capability <ref> embedding` parses.
- invalid capability produces a clap usage error.
- import/pull output warning is emitted for `default-chat` fallback.

REST tests:

- `PATCH /v1/models/{ref}` updates capability and returns model DTO.
- invalid PATCH capability returns `400 bad_request`.
- import/pull mutation responses include a warning when fallback remains
  `default-chat`.
- import/pull job path records `warning_summary` for fallback.

Python helper tests:

- snapshot result JSON includes compact metadata fields from a mocked
  `model_info()`.
- missing optional metadata fields keep backward-compatible output parsing.

Verification commands:

```bash
cargo test -p tentgent-kernel model
cargo test -p tentgent-cli model
cargo test -p tentgent-daemon model
cargo check --workspace
```

Python helper tests should run through the existing Python subproject test
command once its test harness is present for this helper path.

## Rollout Notes

- M2 can ship independently of M3 server compatibility gates because it changes
  only metadata classification and correction.
- M3 should treat M2's capability metadata as input for endpoint compatibility,
  not as backend readiness proof.
