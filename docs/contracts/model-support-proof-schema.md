# Model Support Proof Schema

This document defines the minimum record schema used to explain model support
status. It is the local proof schema introduced for support status and extended
by the `v0.8.0` runtime-profile gate.

Support proof records and support hint records are separate:

- A local proof records an observed result on this machine.
- A support hint records built-in, curated, or shared knowledge about what
  should or should not work.

Effective support status is derived from these records by
[model-support-status.md](./model-support-status.md). Neither record type is
itself the final status resolver.

## Record Kinds

| Kind | Owner | Can produce | Cannot produce |
| --- | --- | --- | --- |
| Local proof | Local machine/runtime event | `verified`, `failed`, or effective `stale` | `supported`, `unknown` |
| Support hint | Tentgent built-in data, curated fixtures, or future shared registry | `supported`, `unsupported`, or effective `stale` | `verified`, `failed` |

Local proof wins over positive support hints when it applies to the same tuple.
Hard unsupported rules and negative support hints can still block routing before
runtime dispatch.

## Local Proof Record

Local proof records are currently stored as TOML under the canonical model
directory in two compatible locations.

Tuple-aware support proofs are stored under:

```text
models/store/<model_ref>/support-proofs/<capability>/<proof_key>.toml
```

The current proof key is derived from:

- `primary_format`
- `runtime_family` when present
- `backend`
- `runtime_version` when present
- `runtime_profile` when present
- `runtime_profile_version` when present

This allows multiple backend or runtime proofs for the same model capability to
coexist. Saving another proof for the same tuple replaces that tuple proof.

The legacy latest-proof location is still written and read for compatibility:

```text
models/store/<model_ref>/capability-proofs/<capability>.toml
```

That path stores only one latest proof per capability. It must not be treated
as the durable tuple index when multiple backend, runtime, adapter, or shape
proofs exist for the same capability.

Capability proof clearing is intentionally capability-wide in this schema
generation. Clearing one `model_ref + capability` removes all tuple-aware
support proof files for that capability and the legacy latest-proof file. It
does not remove model content, stored capability metadata, or proof records for
other capabilities.

The `v0.7.0` schema should be versioned:

```toml
schema_version = 1
record_kind = "local-proof"

model_ref = "<model_ref>"
short_ref = "<short_ref>"
source_kind = "huggingface"
source_repo = "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
source_revision = "<resolved_revision>"

capability = "chat"
primary_format = "mlx"
quantization = "4bit"
backend = "mlx"
runtime_family = "mlx-lm"
runtime_package = "mlx-lm"
runtime_version = "0.24.0"

platform = "macos"
device_class = "apple-silicon"

status = "verified"
proof_source = "server-start"
checked_at = "2026-06-12T00:00:00Z"

[input_shape]
family = "chat"
modalities = ["text"]
provider_shape = "native"

[output_shape]
family = "chat"
modalities = ["text"]
streaming = true
```

Optional fields:

```toml
server_ref = "<server_ref>"
adapter_ref = "<adapter_ref>"
runtime_profile = "mlx-lm-chat-v1"
runtime_profile_version = 1
error_code = "runtime_failed"
error = "backend failed to load model"
```

The JSON representation should use the same field names.

## Required Local Proof Fields

Every local proof must include:

- `schema_version`
- `record_kind = "local-proof"`
- `model_ref`
- `capability`
- `primary_format`
- `backend`
- `runtime_family` when a concrete runtime family is selected
- `platform`
- `device_class`
- `status = "verified" | "failed"`
- `proof_source = "manual-probe" | "server-start" | "endpoint-smoke" |
  "runtime-execution"`
- `checked_at`

The first implementation may preserve older records that only contain the
current `ModelCapabilityProof` fields. A resolver should treat missing
dimension fields as less specific evidence and mark them `stale` when a precise
comparison is required.

Current CLI diagnostics read `runtime_profile` and `runtime_profile_version`
from persisted proof records when a server launch recorded them. Local
server-start proof recording must include these fields whenever the selected
server spec has runtime profile metadata. Server-bound inspection may also pass
the selected server runtime profile into the resolver before comparing support
evidence. `execution_backend` is derived from the proof tuple's
backend/runtime-family fields.

## Identity Fields

Identity fields explain which model the record applies to:

- `model_ref`: canonical content identity.
- `short_ref`: display-only convenience.
- `source_kind`: `huggingface`, `local`, or future source kinds.
- `source_repo`: source repository or local source label when known.
- `source_revision`: resolved immutable source revision when known.

`model_ref` is the authority. Source fields explain provenance and help support
hints target known public fixtures, but source fields must not replace
`model_ref` for local proof matching.

## Proof Key

The proof matching key is the normalized tuple that decides whether a record
applies to the current route:

- `model_ref`
- `capability`
- `primary_format`
- `quantization`
- `backend`
- `runtime_family`
- `runtime_package`
- `runtime_version`
- `runtime_profile`
- `runtime_profile_version`
- `platform`
- `device_class`
- `adapter_ref`
- `input_shape`
- `output_shape`

The current tuple-aware local proof store derives its path key in memory from
the fields it records today: primary format, runtime family, backend, runtime
version, runtime profile, and runtime profile version. A later expanded proof
key must continue to derive from these fields and must not ignore adapter or
shape differences once those fields are recorded.

## Runtime Tuple Fields

Runtime tuple fields explain how the model was run:

- `capability`
- `primary_format`
- `quantization`
- `backend`
- `runtime_family`
- `runtime_package`
- `runtime_version`
- `runtime_profile`
- `runtime_profile_version`
- `platform`
- `device_class`
- `adapter_ref`

If any recorded tuple field changes and the old proof cannot safely apply to
the new tuple, the effective status should become `stale`.

Local model-bound server starts record `server-start` proofs after launch
success or failure. Those records include the selected runtime profile id and
version when the server spec has one. Runtime launch errors are normalized for
display: multi-line output is compacted, common secret environment variable
names are redacted, and long messages are truncated.

Direct local runtime attempts record `runtime-execution` proofs after model
resolution and runtime dispatch. These records are for concrete execution
outcomes, not model lookup, request validation, unsupported input, or cloud
provider failures.

## Input And Output Shape

Input and output shapes describe what the proof exercised. They are not a full
API transcript.

`input_shape` should include:

- `family`: endpoint family such as `chat`, `embedding`, `rerank`,
  `vision-chat`, `audio-transcription`, `audio-speech`,
  `video-understanding`, or `image-generation`
- `modalities`: one or more of `text`, `image`, `audio`, `video`, or `file`
- `provider_shape`: `native`, `openai`, `claude`, `gemini`, or another
  provider adapter name
- optional normalized limits such as context length, image size, audio format,
  or embedding input type

`output_shape` should include:

- `family`
- `modalities`
- optional `streaming`
- optional output format hints such as `json`, `wav`, `png`, or embedding
  vector dimensions

Shape fields are part of stale comparison when the route depends on them.
For example, a text-only chat proof does not verify an image+text vision route.

## Support Hint Record

Support hints should be stored separately from local proof. A hint can be
shipped with Tentgent, generated from curated fixture docs, or loaded from a
future shared registry.

Minimal hint shape:

```toml
schema_version = 1
record_kind = "support-hint"

source = "built-in"
hint_id = "mlx-community-qwen2-5-0-5b-instruct-4bit-chat"
status = "supported"

source_kind = "huggingface"
source_repo = "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
source_revision = "*"

capability = "chat"
primary_format = "mlx"
quantization = "4bit"
backend = "mlx"
runtime_family = "mlx-lm"
platform = "macos"
device_class = "apple-silicon"

reason = "curated smoke fixture"
recorded_at = "2026-06-12T00:00:00Z"

[input_shape]
family = "chat"
modalities = ["text"]
provider_shape = "native"

[output_shape]
family = "chat"
modalities = ["text"]
```

Hint `status` is limited to:

- `supported`
- `unsupported`

Hints must not claim `verified` or `failed`; only local proof can do that.

Hints may omit shape fields only when the support claim truly applies to every
shape that the capability can expose. Otherwise, hints should include
`input_shape` and `output_shape` so a text-only hint does not authorize a
multimodal route.

## Stale Comparison Keys

The resolver should compare these keys when deciding whether proof or hints are
current:

- `schema_version`
- `model_ref` for local proof
- `source_kind`, `source_repo`, and `source_revision` for hints
- `capability`
- `primary_format`
- `quantization`
- `backend`
- `runtime_family`
- `runtime_package`
- `runtime_version`
- `runtime_profile`
- `runtime_profile_version`
- `platform`
- `device_class`
- `adapter_ref`
- `input_shape`
- `output_shape`

Missing keys in old records should not crash resolution. They should reduce
confidence and may produce effective `stale` when the missing dimension is
needed to trust the record.

## Current Compatibility

The current `ModelCapabilityProof` domain type already stores a small subset:

- `model_ref`
- `capability`
- `status`
- `source`
- `primary_format`
- `mlx_runtime_family`
- `backend`
- `runtime_version`
- `server_ref`
- `checked_at`
- `error`

This subset remains readable. The `v0.7.0` implementation can migrate in place
or write expanded records while accepting old records as legacy proof evidence.
