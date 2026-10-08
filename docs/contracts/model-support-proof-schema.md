# Model Support Proof Schema

This contract distinguishes implemented legacy proof records from the complete
proof v2 foundation added by #127. Historical `v0.7.0` examples describing
platform, shape, or a schema marker were design targets, not fields present in
the old persisted `ModelCapabilityProof`.

See [compatibility-tuple-v2.md](./compatibility-tuple-v2.md) for the complete
tuple, observation scope, versioned key, and v2 record contract. Effective
status belongs to [model-support-status.md](./model-support-status.md);
coordination belongs to [resource-blockers.md](./resource-blockers.md).

## Evidence Kinds

| Kind | Meaning | Result |
| --- | --- | --- |
| Local proof | An observed local event, with its actual evidence scope. | Stored `verified` or `failed`; effective `stale` when it cannot answer the query. |
| Support hint | Built-in or curated knowledge, not a local observation. | `supported` or `unsupported`, never local `verified` or `failed`. |
| Legacy manual probe | Capability metadata check without executing a model. | Retains the old partial API result; never becomes a complete v2 execution proof. |

An applicable local failure cannot be hidden by a positive hint. No stored
status overrides hard incompatibility. Evidence is not a runtime ownership
claim or a model-deletion blocker.

## Three File Generations

All paths are relative to the canonical model directory.

| Generation | Path | Identity |
| --- | --- | --- |
| Legacy latest | `capability-proofs/<capability>.toml` | One latest partial result per capability. |
| Tuple-aware v1 | `support-proofs/<capability>/<proof_key>.toml` | Existing format/runtime/backend/profile subset. |
| Complete v2 | `support-proofs/v2/<capability>/<tuple_sha256>.toml` | Complete normalized tuple including observation scope. |

The first two generations have the same legacy body; calling the tuple-aware
generation "v1" does not imply it contains `schema_version = 1`.
Keep their filenames and field encoding readable. Do not rename old files to
new hashes or infer missing facts from the current machine.

A v1 writer saves the tuple-aware support file first, then the latest mirror.
When both exist for the same partial key, the support file is authoritative.
Different partial keys coexist; the latest path is not their durable index.

V2 writes only v2. Never project a complete proof into the less-specific latest
or v1 shape. Legacy gate/list adapters continue reading their compatible
generations until the separate #128/#129 integration. The new evidence API can
read all three generations and retains their provenance and missing dimensions.

## Implemented Legacy Body

Required fields:

- `model_ref`, `capability`, `primary_format`, `backend`;
- `status = "verified" | "failed"`;
- `source = "manual-probe" | "server-start" | "endpoint-smoke" |
  "runtime-execution"`;
- `checked_at`.

Optional fields:

- `mlx_runtime_family`;
- `runtime_version`;
- `runtime_profile` and `runtime_profile_version`;
- `server_ref`;
- `error`.

The existing partial path key contains format, MLX runtime family, backend,
runtime version, profile id, and profile version. The directory supplies model
and capability. Its historical escaping is preserved for old-file lookup, not
reused for new v2 identity.

Missing platform, device, quantization, adapter, or observation facts remain
missing. Legacy records are not malformed merely because they lack v2 fields.
A precise comparison treats incomplete evidence conservatively; old public
response fields and partial gate behavior do not change as a side effect.

## Producer Boundaries

Current producers remain on their existing legacy schema until they can supply
complete authoritative facts. The persistence upgrade does not move when an
event is recorded:

- Manual verify checks stored capability metadata; it does not load a model.
- Local and Cluster eager workers record `server-start` only after confirmed
  terminal preload success or accepted-task load failure. CLI/REST callers
  must not duplicate the worker's record.
- Process launch, lazy start, readiness observation expiry, transport failure,
  and missing/stale Python endpoints do not create preload proof.
- Resolved direct local attempts record `runtime-execution` after dispatch.
  Model lookup, request validation, unsupported input, and Cloud provider
  failures are not local runtime evidence.
- Preload observes loading only. It does not verify inference, streaming,
  provider formatting, adapter execution, or every image workflow.
- Training success is provenance, not serving verification.

The selected runtime profile is an execution profile, not the Python bootstrap
dependency profile. Runtime facts must describe the selected or reused worker,
not an unrelated caller environment. Do not invent a package version to make
an old event satisfy the v2 writer.

## Persistence And Clearing

All generations use the existing atomic replacement primitive and short proof
transactions. The complete lock set and authoritative metadata reread are
specified in [resource-blockers.md](./resource-blockers.md#proof-transactions).

Atomicity is per file, not an all-or-nothing promise for legacy dual writes or
bulk clear. A mirror failure may follow a committed support file. Return an
explicit error, preserve readable committed evidence, and allow safe retry.
A directory-sync failure can likewise occur after a complete replacement.

Exact v2 removal deletes only the selected v2 file. Older evidence remains
older evidence and cannot restore exact `verified`/`failed`.
Capability-wide clear removes all three generations for the selected model and
capability, without deleting assets or other capabilities. Count logical v2
keys plus deduplicated v1/latest records from the same locked snapshot.
Partial failure must not report a successful removal count.

Old binaries ignore v2 and cannot clear it. After an old-version clear,
re-upgrading may expose the untouched v2 record again. No semantic rollback,
automatic backfill, tombstone protocol, or concurrent old/new writer support is
promised. See the [v2 compatibility boundary](./compatibility-tuple-v2.md).

## Error Summaries

Proof errors are diagnostic summaries, not raw logs or request transcripts.
Before persistence and when reading existing proofs, compact whitespace,
redact common credential assignment/header values, and truncate to 500 Unicode
characters plus an optional `...`. The formatter does not read secret stores
or environment values and is not a general PII detector.

Reading sanitizes the returned value without rewriting historical files.
Malformed-proof errors identify the file and failure class, not raw TOML
content. V2 shape fields cannot contain arbitrary payloads or headers.

## Support Hints

The implemented in-memory hint contains capability, status, reason, and
optional format, MLX runtime family, and backend constraints. The built-in
catalog supplies source-aware matching separately.

A future shared registry may define a richer serialized schema, but #127
does not introduce it. Do not interpret older aspirational registry TOML
examples as files that Tentgent currently persists or consumes. A hint never
becomes a local exact proof merely because its descriptive fields match.
