# Models And Support Evidence

Import a complete local model or pull a Hugging Face snapshot. Prepare the [runtime](./runtime.md) and any required [HF credentials](./auth.md) first. Short refs must be unique; use full refs in stored definitions.

## Examples And Common Operations

Import a local model:

```bash
tentgent model add /path/to/local-model
tentgent model add ./models/bge-small --capability embedding
```

Pull models from Hugging Face:

```bash
tentgent model pull google/gemma-3-1b-it
tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
tentgent model pull BAAI/bge-reranker-base --capability rerank --revision main
```

`--capability` accepts `chat`, `embedding`, `rerank`,
`audio-transcription`, `audio-speech`, `vision-chat`, `video-understanding`, or
`image-generation`. Chat, embedding, rerank, audio transcription, audio speech,
vision chat, video understanding, and image generation endpoints enforce this
metadata before runtime dispatch.
`audio-transcription` is available through `tentgent transcribe` and the daemon
job API for local safetensors ASR models. `audio-speech` is available through
`tentgent speak` and daemon `POST /v1/audio/speech/job` for local Transformers
text-to-speech models with `wav` output. MLX audio text-to-speech is available
through the direct Python model-runtime path and still needs Rust daemon job
routing.
`vision-chat` is available through `tentgent vision chat` and daemon
`POST /v1/vision/chat` for local safetensors image-plus-text models.
`video-understanding` is available through `tentgent video understand` and
daemon `POST /v1/video/understanding/job` for local video-plus-text models.
`image-generation` is available through
`tentgent image generate` and daemon `POST /v1/images/generations/job` for
local Diffusers text-to-image models and Apple Silicon MFLUX `mlx-diffusion`
models.

List and inspect models:

```bash
tentgent model catalog
tentgent model catalog --capability <capability> --publisher <publisher>
tentgent model catalog --support-level <support-level>
tentgent model catalog --local --capability <capability>
tentgent model ls
tentgent model inspect <model-ref-or-prefix>
tentgent model rm <model-ref-or-prefix>
tentgent model capability show <model-ref-or-prefix>
tentgent model capability set <model-ref-or-prefix> <capability> [<capability>...]
tentgent model capability add <model-ref-or-prefix> <capability> [<capability>...]
tentgent model capability remove <model-ref-or-prefix> <capability> [<capability>...]
tentgent model capability verify <model-ref-or-prefix> <capability>
tentgent model capability proofs <model-ref-or-prefix>
tentgent model capability proof clear <model-ref-or-prefix> <capability>
```

`<model-ref-or-prefix>` is either the full `model_ref` or a unique short
prefix. `<capability>` accepts `chat`, `embedding`, `rerank`,
`audio-transcription`, `audio-speech`, `vision-chat`, `video-understanding`, or
`image-generation`. `<publisher>` filters catalog rows by publisher name, and
`<support-level>` accepts catalog support levels such as fixture-supported
levels shown by `tentgent model catalog -h`. The optional `[<capability>...]`
tail means `set`, `add`, and `remove` can accept more than one capability in
one command.

`model catalog` lists the built-in model-family support catalog before models
are pulled into the local store. Use `--capability`, `--publisher`,
`--support-level`, `--local`, and `--query` filters to narrow the list. Rows
are followed by a pull command template and descriptions for the capabilities
present in the filtered results.
`model ls` keeps the table compact and prints an `Inspect:` command template
below the table. The list also shows a compact support summary such as
`supported chat`, `unknown embedding`, or `failed chat (+1)` when a model has
multiple capability rows. The list omits long source revision hashes; full
source revision and detailed capability support status are shown by
`model inspect` inside the Field/Value table as compact multi-line rows instead
of a wide capability table.
`model inspect` also shows a `catalog` row when built-in model-family records
match the stored source metadata. Catalog matches identify curated fixtures and
major model families, but `verified` and `failed` support statuses still come
only from local proof records.

When no explicit capability or confident Hugging Face metadata is available,
Tentgent keeps the backward-compatible `chat` default and prints a warning.
Use `model capability set`, `add`, or `remove` to correct stored metadata later
without changing `model_ref`. Capability mutations are canonicalized, de-duped,
and rejected when they would leave the model with no capabilities. The legacy
`model set-capability` command remains as a compatibility alias for replacing
the whole capability set with one value.
For MLX models, inspect output also shows `mlx_runtime_family` when the stored
capability maps to a specific MLX runtime family such as `mlx-lm`,
`mlx-vlm`, `mlx-audio`, or `mlx-diffusion`.
Capability proof commands read and write local tuple-aware support proof
records while preserving the legacy latest proof file for compatibility.
Manual `verify` checks stored metadata and backend labeling; local model-bound
server starts record `server-start` proofs after launch success or failure, and
direct local runtime attempts record `runtime-execution` proofs after chat,
embedding, or rerank execution succeeds or fails.
Launch-derived proofs include the selected runtime profile id and version when
the server spec has one, so a later profile version is treated as new evidence
instead of silently reusing the old proof.
Use `model capability proof clear <model-ref-or-prefix> <capability>` after
fixing a runtime problem to remove stale local `verified` or `failed` proof
evidence for that capability, then retry the route or rerun verification. This
command clears all local proof records for that model capability, including
tuple-aware backend/runtime-profile records and the legacy latest-proof file.
It does not remove stored capability metadata or model content. The daemon REST
API exposes the same recovery action with
`DELETE /v1/models/{reference}/capabilities/proofs/{capability}`.
`tentgent doctor` also reports local model support summaries as capability
checks. Local model-bound server starts now use the same support status as a
startup gate: `verified` and `supported` are allowed by default, `failed` and
`unsupported` are blocked, and `unknown` or `stale` require an explicit
`--allow-unverified` retry.
When stored clusters exist, `doctor` also adds a compact `cluster readiness`
check. It reports whether clusters are ready, partial, blocked, or unknown and
points to `tentgent cluster inspect <cluster-ref>` for route-level details.
Detailed support diagnostics are intentionally kept out of `model ls`.
`model inspect <model-ref>` shows each capability as a multi-line detail row
with `runtime_profile`, `execution_backend`, proof or hint evidence, failure or
stale reason, and a copyable `next_action` when the tuple needs operator work.
`model inspect` also reports stored model-file diagnostics. Missing runtime
required files such as GGUF weights, `config.json`, tokenizer assets,
processor/preprocessor metadata, or Diffusers `model_index.json` are shown with
the checked path and a recovery action. Tentgent does not silently create or
patch missing model files; remove the corrupted model entry, then pull or
import the model again from a complete source.

For recommended small Hugging Face fixtures, gated-access reminders, and
copy-paste smoke commands, see [model-fixtures.md](./model-fixtures.md).
`model rm` deletes the managed copy and its indexes. Referencing resources can block removal; inspect the reported blockers and remove unused references before retrying. It does not delete the original import directory.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `model add/pull` | `--capability <CAPABILITY>` | Capability metadata to assign to this model |
| `model pull` | `-r, --revision <REV>` | Hugging Face revision to resolve before downloading |
| `model catalog` | `--capability <CAPABILITY>` | Filter by Tentgent endpoint capability |
| `model catalog` | `--publisher <PUBLISHER>` | Filter by publisher text, case-insensitive |
| `model catalog` | `--support-level <LEVEL>` | Filter by support level: fixture-supported, local-runtime-supported, catalog-known, requires-external-runtime, known-unsupported, deprecated |
| `model catalog` | `--local` | Show only entries that can become local support hints |
| `model catalog` | `-q, --query <TEXT>` | Search publisher, family, source, tags, and recommendation text |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/models` | None. |
| `GET` | `/v1/models/{reference}` | None. |
| `DELETE` | `/v1/models/{reference}` | None. |
| `POST` | `/v1/models/{reference}/capabilities` | `{"set":["<capability>"]}` or `{"add":["<capability>"],"remove":["<capability>"]}`. |
| `GET` | `/v1/models/{reference}/capabilities/proofs` | None. |
| `DELETE` | `/v1/models/{reference}/capabilities/proofs/{capability}` | None. |
| `POST` | `/v1/models/{reference}/capabilities/verify` | `{"capability":"<capability>"}`. |
| `PATCH` | `/v1/models/{reference}` | Legacy compatibility alias for replacing the capability set with one `{"capability":"..."}` value. |
| `POST` | `/v1/models/import` | `{"path":"/absolute/model-dir","capability":"optional-capability"}` |
| `POST` | `/v1/models/pull` | `{"repo_id":"org/model","revision":"optional","capability":"optional-capability"}` |
| `POST` | `/v1/models/import/jobs` | Same as `/v1/models/import`, returns a job. |
| `POST` | `/v1/models/pull/jobs` | Same as `/v1/models/pull`, returns a job. |

Capability mutations canonicalize and de-duplicate values, set
`model_capability_source` to `manual-update`, and reject requests that would
leave a model with no capabilities.
`<capability>` accepts `chat`, `embedding`, `rerank`, `audio-transcription`,
`audio-speech`, `vision-chat`, `video-understanding`, or `image-generation`.

Capability proofs are latest local records keyed by model and capability.
Manual `verify` is a metadata-level probe in this slice; local model-bound
server starts also write `server-start` proofs after launch success or failure,
and direct local runtime attempts write `runtime-execution` proofs after
execution succeeds or fails.
Deleting a capability proof path clears all local proof records for that model
capability, including tuple-aware backend/runtime-profile records and the
legacy latest-proof file, without changing model content or capability
metadata. The delete response includes `proof_clear.capability` and
`proof_clear.removed_proof_count`.

### Import And Read Results

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/models/import \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"path":"/absolute/path/on/daemon-host/model","capability":"chat"}'
curl -sS 'http://127.0.0.1:8790/v1/models/<model-ref>' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

List returns `models`; inspect returns `model`. Import/pull responses contain
`model`, `mutation`, and `warnings`; take the managed ref from `model.model_ref`.
Source paths belong to the daemon host and must be absolute. Large synchronous
imports/pulls may outlive short HTTP timeouts; `/jobs` variants return a
[job](./jobs.md) immediately. Capability updates return the updated `model`
and `mutation`; verification adds `proof`, while proof listing returns `proofs`.
Guarded mutations return structured [blockers](./api.md) when resources are in use.

## Related Guides

[Small fixtures](./model-fixtures.md) · [Support catalog](./model-support-catalog.md) · [Inference](./inference/README.md) · [Adapters](./adapters.md)
