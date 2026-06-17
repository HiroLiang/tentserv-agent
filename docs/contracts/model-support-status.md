# Model Support Status

This document defines the support status vocabulary used by model and runtime
workflows. It is a design contract for the `v0.7.0` support-status track.

Support status is derived from evidence. It is not the same thing as stored
`model_capabilities` and it is not the same thing as raw capability proof
records.

The local proof and support hint record schema is defined in
[model-support-proof-schema.md](./model-support-proof-schema.md).

## Purpose

Tentgent needs one stable answer for whether a model can serve a capability on
this machine. Later code should be able to map every
`model + capability + backend/runtime + platform` tuple to exactly one status
before starting a server or dispatching a job.

The status should explain:

- whether Tentgent knows the tuple is supported;
- whether this machine has verified the tuple;
- whether a recent attempt failed;
- whether older evidence is stale;
- whether the tuple is unknown and needs an explicit policy decision.

## Tuple Scope

A support status applies to one resolved tuple. It must not be keyed by model
name alone.

The tuple should include at least:

- `model_ref`
- capability, such as `chat`, `embedding`, or `vision-chat`
- primary model format, such as `mlx`, `safetensors`, `gguf`, or `diffusers`
- backend or runtime family, such as `mlx-lm`, `mlx-vlm`, `mlx-audio`,
  `mlx-diffusion`, `transformers`, or `llama-cpp`
- runtime package or adapter version when known
- platform and device class, such as macOS Apple Silicon, Linux CUDA, or CPU
- source identity and revision when a built-in or shared support record depends
  on source metadata

If any tuple dimension changes in a way that can affect execution, previous
proof for the old tuple must not be treated as proof for the new tuple.

## Status Vocabulary

| Status | Meaning | Typical routing behavior |
| --- | --- | --- |
| `verified` | Local proof confirms this tuple worked on this machine. | Prefer this route. Allow server start or job dispatch unless another gate fails. |
| `failed` | The latest applicable local proof for this tuple failed. | Block by default and show the recorded failure. |
| `supported` | Built-in, curated, or shared support evidence says this tuple should work, but this machine has not verified it yet. | Allow when policy accepts hinted support, and encourage or run verification. |
| `unknown` | No applicable positive proof, negative proof, or support record exists. | Require explicit allow-unknown policy before trying. |
| `unsupported` | Built-in rules, support records, or hard compatibility checks say this tuple cannot work. | Block before runtime dispatch. |
| `stale` | Earlier evidence exists, but it no longer applies to the current tuple or policy version. | Require re-verification or re-resolution before treating it as supported. |

`verified` and `failed` can be persisted as proof statuses. The other support
statuses are effective statuses derived by a resolver from proof, support
records, metadata, policy, and environment state.

## Operator Next Actions

| Status | Next action |
| --- | --- |
| `verified` | Prefer the tuple. No support-specific action is required. |
| `supported` | Try the route or run a local smoke verification when confidence matters. |
| `failed` | Inspect the local proof, fix the runtime/backend/input issue, clear the failed proof, then rerun the smoke verification. |
| `unsupported` | Pick a different model, capability, backend, or route. Do not retry the same tuple without changing evidence. |
| `unknown` | Add an explicit capability/support record, choose an allow-unknown policy, or verify the tuple before relying on it. |
| `stale` | Refresh the proof by rerunning verification under the current runtime/profile/platform tuple. |

## Evidence Sources

Support status may use these evidence sources:

- Declared model capability metadata:
  `default-chat`, `explicit-user`, `huggingface-metadata`, or
  `manual-update`.
- Local proof records:
  `manual-probe`, `server-start`, or `endpoint-smoke`.
- Built-in support records shipped with Tentgent.
- Future shared or downloaded support records.
- Runtime profile availability and backend compatibility rules.
- Platform readiness, such as required Python package, backend, device, or
  system dependency availability.
- User policy, such as allowing unknown tuples or overriding local routing.

Declared capability metadata says which endpoint family a model is intended to
serve. It is necessary routing input, but it does not prove runtime support.

## Built-In Model Catalog

Tentgent ships a built-in model support catalog for curated fixture models and
major public model families. The catalog records source identity, publisher,
family, approximate scale, endpoint capabilities, descriptive tags, support
level, evidence source, and runtime notes.

The catalog is source-aware. Exact Hugging Face repository matches take
precedence over family patterns such as `mlx-community/Qwen*`. This prevents a
fixture record from accidentally proving every model in the same family.

Catalog levels map to resolver evidence conservatively:

- `fixture-supported` and `local-runtime-supported` can produce `supported`
  hints.
- `known-unsupported` can produce `unsupported` hints.
- `catalog-known`, `requires-external-runtime`, and `deprecated` are displayed
  as catalog context but do not prove local support.

Large model records such as 70B, 120B, 235B, or larger MoE families should
usually be marked `catalog-known` or `requires-external-runtime` unless a
local runtime tuple is explicitly known to work. These records help users
recognize model families without implying that the current machine can serve
them.

## Precedence

When multiple evidence sources apply, resolve status in this order:

1. Hard incompatibility returns `unsupported`.
   Examples include a missing required model capability, a backend that cannot
   serve the requested capability, or a platform/device class that the runtime
   family cannot use.
2. Staleness checks run before trusting proof or support records.
   If the best applicable evidence is stale and no newer applicable evidence
   exists, return `stale`.
3. The latest applicable local proof wins over support hints.
   A successful proof returns `verified`; a failed proof returns `failed`.
4. Built-in or shared negative support records return `unsupported`.
5. Built-in or shared positive support records return `supported`.
6. User capability metadata and user allow-unknown policy can permit an
   attempt, but they do not create `verified` or `supported` by themselves.
7. If no evidence applies, return `unknown`.

A local `failed` proof must not be hidden by a `supported` hint. The operator
should see that the tuple was expected to work but failed locally.

## User-Facing Surfaces

Support status and runtime-profile diagnostics are surfaced for visibility
before they become a hard routing gate. Current CLI surfaces should keep output
compact and preserve existing command behavior:

- `tentgent model ls` may show the most actionable support summary for each
  model, with `model inspect` as the detailed view.
- `tentgent model inspect` should show per-capability status, evidence,
  runtime profile, backend tuple, short reason, and next action when the tuple
  needs operator work.
- `tentgent server inspect` should show local bound-model support for the
  selected server capability. This includes the selected runtime profile and
  execution backend when the server is bound to a local model. Cloud provider
  servers are outside local model proof scope.
- `tentgent doctor` may warn about missing proof, stale proof, failed proof,
  unsupported, or unknown tuples. The main check list should stay compact;
  long failure, backend, profile, and next-action details belong in the detail
  block.

Runtime profiles identify selected local server execution profiles such as
`local-chat-mlx-v1` or `local-embedding-transformers-peft-v1`. They are not the
same as managed Python bootstrap dependency profiles such as `local-model`.
Execution backend labels, such as `mlx-lm`, `mlx-vlm`, `mlx-audio`,
`mlx-diffusion`, `safetensors`, or `diffusers`, identify the backend/runtime
family used for the model tuple. These labels are diagnostics, not provider API
route names.

When a status is actionable, CLI diagnostics should provide a copyable command:

- `failed` or `stale`: clear the local proof before retrying the route.
- `unknown`: record or run a verification flow before relying on the tuple.
- `unsupported`: add missing capability metadata only when the model does not
  declare the requested capability; otherwise inspect the model and choose a
  different tuple.

Local model-bound server starts use this status as a hard startup gate.
`verified` and `supported` are allowed by default. `failed` and `unsupported`
are blocked. `unknown` and `stale` are blocked unless the caller sets an
explicit allow-unverified policy, currently exposed by CLI and REST server
start flows as `allow_unverified`. Cloud provider servers are outside local
model proof scope and keep using provider capability checks.

Endpoint smoke verification remains separate runtime proof work. A gate
decision may allow a `supported` or explicitly allowed `unknown`/`stale` tuple
to launch, but that decision does not by itself create a `verified` proof. The
actual local server launch outcome records a `server-start` proof: successful
starts write `verified`, and launch failures after profile selection write
`failed`.

## Stale Evidence

Evidence becomes stale when one of the tuple dimensions or resolver assumptions
changes enough that the old conclusion may no longer be true.

Proof should become stale when any of these change:

- proof or hint `schema_version`
- `model_ref`
- capability
- primary model format
- quantization
- backend or runtime family
- runtime package or adapter version, when recorded or required by the route
- platform or device class
- input or output shape
- selected adapter
- relevant runtime profile version
- support-status resolver schema version
- support record version that supplied the previous conclusion

Proof may also become stale when required runtime files, adapters, or model
variants are removed.

`stale` is an effective status. The proof record may remain stored for audit
history, but a resolver must not use stale proof as current `verified` or
`failed` evidence.

## Transition Rules

| From | Event | To |
| --- | --- | --- |
| `unknown` | Positive built-in or shared support record is added. | `supported` |
| `unknown` | Negative built-in or shared support record is added. | `unsupported` |
| `unknown` | Explicit allow-unknown attempt succeeds and records proof. | `verified` |
| `unknown` | Explicit allow-unknown attempt fails and records proof. | `failed` |
| `supported` | Local verification succeeds. | `verified` |
| `supported` | Local verification or runtime attempt fails. | `failed` |
| `verified` | Newer applicable local attempt fails. | `failed` |
| `failed` | Newer applicable local attempt succeeds. | `verified` |
| `verified` or `failed` | Tuple, runtime profile, platform, or resolver schema changes. | `stale` |
| `stale` | Re-verification succeeds. | `verified` |
| `stale` | Re-verification fails. | `failed` |
| `supported` | Support record changes and no current local proof applies. | `stale` or `unsupported` |
| any status | Hard incompatibility is discovered for the current tuple. | `unsupported` |

## Output Requirements

Any user-facing or API-facing support status should include enough context to
explain the decision:

- effective status;
- model reference and short reference;
- requested capability;
- selected backend/runtime family;
- evidence source that produced the status;
- whether local proof was used;
- stale reason when status is `stale`;
- failure message when status is `failed`;
- next action when status is `unknown`, `unsupported`, `failed`, or `stale`.

The initial implementation may expose a compact view first, but the underlying
resolver should preserve these fields so CLI, daemon, doctor, and server-start
errors can present the same decision.

## Non-Goals

This contract does not define:

- the file format for a future support registry;
- every CLI or HTTP response shape for support status;
- live verification behavior for every capability;
- dynamic routing across multiple candidate backends.

Those are separate implementation slices. This contract fixes the vocabulary,
resolver rules, and local server-start gate policy they should follow.
