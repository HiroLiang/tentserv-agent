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

Replace each `<...-model-ref>` with a full managed model ref or an unambiguous
ref prefix. Routes are optional except that a runnable Cluster currently needs
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
