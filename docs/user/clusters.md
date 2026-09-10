# Clusters

A Cluster is one named local server configuration that routes different AI
workloads to different managed models. It lets one server accept chat,
embedding, rerank, audio transcription, and vision requests without requiring
callers to know each model ref.

Clusters are experimental in `v1.1.0`. Existing direct local-model and cloud
server behavior remains unchanged.

## Define A Cluster

Create a TOML file such as `cluster.toml`:

```toml
schema_version = 1
cluster_ref = "local-assistant"
route_update_policy = "drain"

[routes.chat]
kind = "local-model"
model_ref = "<chat-model-ref>"

[routes.embedding]
kind = "local-model"
model_ref = "<embedding-model-ref>"

[routes.rerank]
kind = "local-model"
model_ref = "<rerank-model-ref>"

[routes.audio-transcription]
kind = "local-model"
model_ref = "<audio-model-ref>"

[routes.vision-chat]
kind = "local-model"
model_ref = "<vision-model-ref>"
```

Replace each `<...-model-ref>` with a full canonical managed model ref from
`tentgent model inspect`; short selectors are not valid in stored route definitions. Routes are optional except that a runnable Cluster currently needs
a local `chat` route. A request for an omitted route receives
`cluster_route_missing`; Tentgent does not silently send it to chat or another
model.

Validate, store, and inspect the definition:

```bash
tentgent cluster validate <cluster-definition.toml>
tentgent cluster apply <cluster-definition.toml>
tentgent cluster ls
tentgent cluster inspect <cluster-ref>
```

`validate` and `apply` do not start model runtimes. `inspect` computes current
route readiness from managed model metadata, runtime profiles, support proofs,
provider metadata, and non-secret auth presence. It also shows safe ownership
summaries and next actions when a route is unknown, stale, failed, unsupported,
or unavailable.

## Run And Manage The Server

Start the Cluster through the normal server lifecycle:

```bash
tentgent cluster run <cluster-ref> --host 127.0.0.1 --port <port> --detach
tentgent server ps
tentgent server inspect <server-ref>
tentgent server stop <server-ref>
tentgent server rm <server-ref>
```

Use `--allow-unverified` only when unknown or stale local support evidence is
acceptable for the launch. It never bypasses failed, unsupported, or
unavailable route state.

The Cluster server maps these request families to the matching configured
route:

| Request family | Cluster route |
| --- | --- |
| `/v1/chat`, `/v1/chat/stream`, OpenAI chat, Claude messages, Gemini generate content | `chat` |
| `/v1/embeddings` | `embedding` |
| `/v1/rerank` | `rerank` |
| `/v1/audio/transcriptions` | `audio-transcription` |
| `/v1/vision/chat` | `vision-chat` |

Provider-shaped request model names are compatibility input only. They cannot
replace the model selected by the Cluster definition.

Managed chat adapters remain request-time choices through the native local
chat `adapter_ref` field. A Cluster does not store a fixed model-plus-adapter
target, and adapter compatibility is checked against the selected chat model.

## Updates, Ownership, And Removal

`cluster apply` replaces the complete stored definition. The currently stored
`route_update_policy` controls target changes:

- `drain` sends new requests to the new route generation while existing
  requests finish on the old generation.
- `block` rejects target-changing replacement. Change only the policy to
  `drain`, then apply the target change separately.

Tentgent coordinates resource transitions across CLI and daemon processes.
Active Cluster routes and stored server specs produce structured blockers so a
model, capability, adapter, Cluster, or server spec cannot be removed while a
conflicting operation still owns it.

Cluster removal never cascades into model, adapter, runtime profile, or server
deletion. Stop and remove every server spec that references the Cluster, let
active requests drain, then run:

```bash
tentgent server ls
tentgent server stop <running-server-ref>
tentgent server rm <server-ref>
tentgent cluster rm <cluster-ref>
```

Stopping a server is not enough: a stopped spec can still be started again and
therefore continues to reference the Cluster. Different ports or launch
settings can create multiple specs for the same Cluster. Remove every blocker
reported by `cluster rm`; Tentgent does not cascade-delete those specs.

Use `tentgent runtime reconcile` to inspect stale ownership records. It is a
dry run unless `--apply` is provided, and it never removes managed model,
adapter, dataset, Cluster, or server content.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `cluster run` | `-H, --home <HOME>` | Optional Tentgent runtime home override for cluster and server state |
| `cluster run` | `-a, --host <HOST>` | Interface for the active HTTP listener; use 127.0.0.1 for loopback. |
| `cluster run` | `-p, --port <PORT>` | Fixed TCP port. Omit to auto-scan from 8780 |
| `cluster run` | `-l, --lazy-load` | Record the shared server lazy-load preference in the stored spec |
| `cluster run` | `-i, --idle-seconds <N>` | Deprecated alias for --runtime-idle-seconds |
| `cluster run` | `--runtime-idle-seconds <N>` | Shut down each managed Python runtime after N workload-idle seconds |
| `cluster run` | `--model-idle-seconds <N>` | Release each loaded local model after N model-idle seconds. Defaults to 0 |
| `cluster run` | `--allow-unverified` | Allow unknown or stale local route support evidence for this launch |
| `cluster run` | `-d, --detach` | Launch the initial cluster server process in background mode |
| `cluster apply/validate/ls/inspect/rm` | `-H, --home <HOME>` | Optional Tentgent runtime home override |
| `cluster apply/validate` | `--force` | Allow reading TOML from an obvious secret-bearing path |

## HTTP API

Start the [daemon](./daemon.md) and follow its [HTTP authentication rules](./api.md).
This example stores one local chat route; replace the ref with its full value:

```bash
curl -sS -X PUT http://127.0.0.1:8790/v1/clusters/local-assistant \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"schema_version":1,"cluster_ref":"local-assistant","routes":{"chat":{"kind":"local-model","model_ref":"<full-chat-model-ref>"}}}'
```

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/clusters` | None. |
| `PUT` | `/v1/clusters/{cluster_ref}` | JSON cluster definition. |
| `GET` | `/v1/clusters/{cluster_ref}` | None. |
| `DELETE` | `/v1/clusters/{cluster_ref}` | None. |

Clusters are named route definitions. `PUT` validates and stores a definition;
it does not start runtimes, verify proofs, read provider secrets, or compute
readiness. `GET /v1/clusters/{cluster_ref}` returns the stored definition plus
read-only readiness fields such as aggregate `readiness.status`, per-route
`readiness.status`, flags, details, and `next_actions[].code`.

Cluster definitions can be launched through the managed server API described in [Servers](./servers.md#http-api). Inference requests go to the resulting cluster server port, not to the
daemon's `/v1/clusters` management routes. The first experimental runtime
dispatches local `chat`, `embedding`, `rerank`, `audio-transcription`, and
`vision-chat` routes; provider targets return an explicit unsupported-target
error.

`DELETE /v1/clusters/{cluster_ref}` returns `409 cluster_in_use` while any
running or stopped server spec or active route claim references the Cluster.
Stopping a server does not remove its reusable spec. Remove every server spec
listed in `blockers` before retrying; deletion never cascades into server specs
or model resources.

## Current Limits

- Provider route targets can be stored and inspected but are not executable by
  the `v1.1.0` Cluster server.
- There is no automatic cross-route fallback or automatic image/audio-to-chat
  context assembly.
- Cluster identity is the validated `cluster_ref`; route targets are internal
  definition fields and do not have separate public refs.
- Runtime ownership remains file-backed. SQLite migration and a broader
  CPU/GPU/memory scheduler are later 1.x work.

See [cluster.md](../contracts/cluster.md) for the exact schema and routing
contract, [runtime-ownership.md](../contracts/runtime-ownership.md) for
lifecycle rules, and [resource-blockers.md](../contracts/resource-blockers.md)
for mutation protection.
