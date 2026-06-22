# API Surface Stability

This contract classifies the Tentgent caller-facing API surface before the
`1.0.0` freeze. It is the source of truth for whether an endpoint, command, or
wire shape is stable enough to document as a long-lived interface.

Provider compatibility status is separate from stability. A provider-shaped
surface can be `Partial` in the user matrix while still having stable route
names and stable unsupported-error codes.

## Stability Tiers

| Tier | Meaning |
| --- | --- |
| `stable` | Public, documented, and intended to remain compatible through `1.0.0` except for additive fields or clearer error messages. |
| `experimental` | Public or documented for current users, but still allowed to tighten behavior, fields, diagnostics, or operational semantics before `1.0.0`. |
| `internal` | Implementation boundary for Tentgent components, hidden commands, workers, or daemon-local state. External callers must not depend on it. |
| `deprecated` | Kept callable for compatibility, but not promoted for new use. Prefer the replacement listed in this contract. |

## Daemon REST Surface

These routes are exposed by `tentgent daemon`.

| Tier | Routes | Notes |
| --- | --- | --- |
| `stable` | `GET /healthz`, `GET /v1/status` | Process liveness, runtime-home summary, and daemon warning records. |
| `stable` | `GET /v1/auth`, `GET /v1/auth/{provider}` | Provider credential presence without secret disclosure. |
| `experimental` | `GET /v1/doctor` | Public diagnostic route. Output remains compact and actionable; warning coverage may expand additively. |
| `stable` | `GET /v1/daemon/logs`, `GET /v1/daemon/logs/stdout`, `GET /v1/daemon/logs/stderr` | Local log diagnostics. Tail query behavior may gain additional guards. |
| `experimental` | `POST /v1/daemon/shutdown` | Public local shutdown request. Active daemon jobs are interrupted before shutdown; workspace cleanup is retention-aware and best-effort. |
| `stable` | `POST /v1/chat`, `POST /v1/embeddings`, `POST /v1/rerank`, `POST /v1/vision/chat` | Native synchronous local inference routes. Backend availability and support gates are capability-specific. |
| `stable` | `POST /v1/chat/completions`, `POST /v1/messages`, `POST /v1beta/models/{*operation}`, `POST /v1/embeddings`, `POST /v1/images/generations` | Provider-shaped daemon ingress routes. Supported fields are partial; known unsupported provider fields, content, operations, and capabilities use stable error codes. |
| `experimental` | `POST /v1/video/understanding/job`, `GET /v1/video/understanding/job/{job_id}/result` | Durable video workflow. Route shape is public; decoder/runtime support remains model and backend dependent. |
| `experimental` | `POST /v1/images/generations/job`, `GET /v1/images/generations/job/{job_id}/files`, `GET /v1/images/generations/job/{job_id}/files/{file_id}` | Durable local text-to-image workflow. Artifact route shape is public; runtime support remains model and backend dependent. |
| `experimental` | `POST /v1/images/transforms/job`, `GET /v1/images/transforms/job/{job_id}/files`, `GET /v1/images/transforms/job/{job_id}/files/{file_id}` | Durable image-to-image workflow. |
| `experimental` | `POST /v1/images/inpaint/job`, `GET /v1/images/inpaint/job/{job_id}/files`, `GET /v1/images/inpaint/job/{job_id}/files/{file_id}` | Durable inpainting workflow. |
| `experimental` | `POST /v1/images/control/job`, `GET /v1/images/control/job/{job_id}/files`, `GET /v1/images/control/job/{job_id}/files/{file_id}` | Durable ControlNet-style workflow. Backend support is narrower than the route shape. |
| `experimental` | `POST /v1/audio/transcriptions/job`, `GET /v1/audio/transcriptions/job/{job_id}/result`, `POST /v1/audio/transcriptions/jobs`, `GET /v1/audio/transcriptions/jobs/{job_id}/result` | Durable audio transcription routes. The singular upload route is preferred for new clients; plural `jobs` routes remain compatibility aliases. |
| `experimental` | `POST /v1/audio/speech/job`, `GET /v1/audio/speech/job/{job_id}/result` | Durable audio speech route. Voice/language support is model-dependent. |
| `experimental` | `GET /v1/jobs`, `GET /v1/jobs/{job_id}`, `DELETE /v1/jobs/{job_id}`, `POST /v1/jobs/{job_id}/cancel` | Public job control surface. Durable cancellation and terminal deletion semantics are documented; already-started blocking worker interruption remains best-effort. |
| `stable` | `GET /v1/models`, `GET /v1/models/{reference}`, `POST /v1/models/import`, `POST /v1/models/pull`, `POST /v1/models/import/jobs`, `POST /v1/models/pull/jobs`, `DELETE /v1/models/{reference}` | Managed model discovery and import/pull surface. Background job behavior follows the job stability notes above. |
| `experimental` | `POST /v1/models/{reference}/capabilities`, `GET /v1/models/{reference}/capabilities/proofs`, `DELETE /v1/models/{reference}/capabilities/proofs/{capability}`, `POST /v1/models/{reference}/capabilities/verify` | Public support-status and proof operations. Retry and stale-state recovery behavior is documented by the model-support contracts. |
| `deprecated` | `PATCH /v1/models/{reference}` | Legacy alias for updating one model capability. Prefer `POST /v1/models/{reference}/capabilities`. |
| `stable` | `GET /v1/adapters`, `GET /v1/adapters/{reference}`, `POST /v1/adapters/import`, `POST /v1/adapters/pull`, `POST /v1/adapters/import/jobs`, `POST /v1/adapters/pull/jobs`, `POST /v1/adapters/{reference}/bind`, `DELETE /v1/adapters/{reference}` | Managed adapter discovery, import/pull, binding, and deletion surface. |
| `stable` | `GET /v1/datasets`, `GET /v1/datasets/{reference}`, `POST /v1/datasets/validate`, `POST /v1/datasets/template`, `POST /v1/datasets/import`, `POST /v1/datasets/import/jobs`, `POST /v1/datasets/{reference}/export`, `POST /v1/datasets/{reference}/diff`, `DELETE /v1/datasets/{reference}` | Managed dataset discovery and deterministic local dataset tools. |
| `experimental` | `POST /v1/datasets/synth/jobs`, `POST /v1/datasets/eval/jobs` | Provider-backed dataset tools. Prompt contracts and provider output diagnostics may tighten before `1.0.0`. |
| `experimental` | `GET /v1/train/lora/plans`, `POST /v1/train/lora/plans/preview`, `POST /v1/train/lora/plans`, `GET /v1/train/lora/plans/{reference}`, `DELETE /v1/train/lora/plans/{reference}` | Managed LoRA plan surface. Plan identity is contracted; training readiness remains model/backend dependent. |
| `experimental` | `POST /v1/train/lora/plans/{reference}/runs`, `GET /v1/train/lora/plans/{reference}/runs`, `GET /v1/train/lora/runs`, `GET /v1/train/lora/runs/{reference}`, `GET /v1/train/lora/runs/{reference}/metrics`, `GET /v1/train/lora/runs/{reference}/logs`, `GET /v1/train/lora/runs/{reference}/logs/raw` | Managed LoRA run surface. Plan identity is contracted; run execution, stale process handling, and diagnostics remain experimental. |
| `stable` | `GET /v1/servers`, `POST /v1/servers`, `GET /v1/servers/{reference}`, `DELETE /v1/servers/{reference}`, `POST /v1/servers/{reference}/start`, `POST /v1/servers/{reference}/stop`, `GET /v1/servers/{reference}/health`, `GET /v1/servers/{reference}/logs`, `GET /v1/servers/{reference}/logs/stdout`, `GET /v1/servers/{reference}/logs/stderr` | Stored local/cloud server registry and lifecycle surface. Runtime profile coverage still expands by capability. |
| `stable` | `GET /v1/sessions`, `POST /v1/sessions`, `GET /v1/sessions/{reference}`, `PATCH /v1/sessions/{reference}`, `DELETE /v1/sessions/{reference}`, `GET /v1/sessions/{reference}/messages`, `POST /v1/sessions/{reference}/messages`, `POST /v1/sessions/{reference}/compact` | Local session metadata and transcript management surface. Compaction is explicit and destructive. |

## Local Model-Bound Server Surface

These routes are exposed by `tentgent server run <model-ref>`.

| Tier | Routes | Notes |
| --- | --- | --- |
| `stable` | `GET /healthz` | Local server process health and bound-model metadata. |
| `stable` | `POST /v1/chat/completions`, `POST /v1/messages`, `POST /v1beta/models/{*operation}`, `POST /v1/embeddings`, `POST /v1/images/generations` | Provider-shaped local ingress routes. The server is bound to one model and rejects incompatible capabilities before runtime execution. |
| `stable` | Native proxied routes supported by the bound capability, including `/v1/chat`, `/v1/chat/stream`, `/v1/embeddings`, and `/v1/images/generations` | The proxy forwards matching native Tentgent bodies to the Python runtime. |
| `internal` | Local proxy fallback behavior for unlisted paths | Fallback forwarding is an implementation convenience, not a public promise that every Python runtime path is exposed. |

## Direct Cloud Server Surface

These routes are exposed by `tentgent server run openai:<model>`,
`tentgent server run anthropic:<model>`, `tentgent server run claude:<model>`,
or `tentgent server run gemini:<model>`.

| Tier | Routes | Notes |
| --- | --- | --- |
| `stable` | `GET /healthz` | Cloud server process health and bound provider model metadata. |
| `stable` | `POST /v1/chat`, `POST /v1/chat/completions`, `POST /v1/messages`, `POST /v1beta/models/{*operation}`, `POST /v1/embeddings`, `POST /v1/images/generations` | Provider-bound route names and supported error codes are stable. Provider family coverage remains partial as documented in the provider compatibility matrix. |

## Python Model Runtime Surface

The Python model runtime is started and managed by Rust. Its routes are mounted
as `/v1/*` today, but they are daemon/server-internal execution routes rather
than caller-facing API surfaces.

| Tier | Routes | Notes |
| --- | --- | --- |
| `internal` | `GET /healthz`, `POST /v1/lifecycle/shutdown` | Rust supervisor health and graceful shutdown boundary for one runtime process. |
| `internal` | `POST /v1/chat`, `POST /v1/chat/stream`, `POST /v1/embeddings`, `POST /v1/rerank` | Direct local inference execution routes called by Rust. |
| `internal` | `POST /v1/audio/transcriptions`, `POST /v1/audio/speech`, `POST /v1/images/generations`, `POST /v1/images/transforms`, `POST /v1/images/inpaint`, `POST /v1/images/control`, `POST /v1/video/understanding`, `POST /v1/vision/chat` | Direct media execution routes. Rust owns upload handling, job workspaces, and public result routes. |
| `internal` | `POST /v1/tuning/lora/runs` | Direct LoRA execution route. Rust owns managed plan identity, durable run records, and adapter import. |

Future `/internal/v1/*` aliases may be added to make this boundary visually
distinct, but external callers must not depend on either the current `/v1/*`
runtime routes or a future alias.

## CLI Surface

The CLI is the primary user entry point. Visible command names are public unless
listed as hidden or deprecated below.

| Tier | Commands | Notes |
| --- | --- | --- |
| `stable` | `tentgent doctor`, `tentgent runtime bootstrap`, `tentgent runtime status` | Runtime and diagnostic commands. Doctor coverage may expand additively. |
| `stable` | `tentgent model add`, `model pull`, `model catalog`, `model ls`, `model rm`, `model inspect`, `model capability show/set/add/remove` | Model management and declared capability commands. |
| `experimental` | `tentgent model capability proofs`, `model capability verify`, `model capability proof clear` | Public support-proof commands. Retry and stale-state recovery behavior is documented by the model-support contracts. |
| `deprecated` | `tentgent model set-capability` | Hidden legacy compatibility command. Prefer `tentgent model capability set`. |
| `stable` | `tentgent chat`, `tentgent embed`, `tentgent rerank`, `tentgent transcribe`, `tentgent speak`, `tentgent vision chat`, `tentgent video understand`, `tentgent image generate/transform/inpaint/control` | One-shot local inference commands. Backend availability remains model/platform dependent. |
| `stable` | `tentgent adapter add/pull/ls/inspect/bind/rm`, `tentgent dataset add/validate/template/ls/inspect/export/diff/rm`, `tentgent store gc` | Managed store, adapter, and deterministic local dataset commands. |
| `experimental` | `tentgent dataset synth`, `tentgent dataset eval` | Provider-backed dataset commands. Prompt contracts and provider output diagnostics may tighten before `1.0.0`. |
| `experimental` | `tentgent train lora plan create/ls/inspect/rm`, `tentgent train lora run` | Public LoRA training commands. Plan identity is contracted; run behavior and stale recovery remain experimental. |
| `internal` | `tentgent train lora run-worker` | Hidden detached worker command. |
| `stable` | `tentgent daemon run/start/status/stop`, `tentgent server run/ls/ps/inspect/start/stop/rm`, `tentgent session ls/inspect/messages/create/update/append/compact/rm`, `tentgent auth status/mode/hf/openai/anthropic/gemini set/rm` | Daemon lifecycle, server registry, session management, and provider auth commands. |
| `internal` | `tentgent __cloud-server-runtime`, `tentgent __local-server-runtime` | Hidden worker entry points for managed server processes. |

Visible aliases such as `embed` / `embedding`, `ls` / `list`, `rm` / `remove`,
and provider auth `status` / `ls` are stable spelling aliases unless a future
release explicitly deprecates them.

## Audit Record

The archived `v0.9.0` audit findings are kept in
[v0.9.0-api-surface-audit-findings.md](../plans/archive/v0.9.0-api-surface-audit-findings.md).
That file records wording gaps, test follow-ups, and behavior changes that were
discovered while preparing this contract.
