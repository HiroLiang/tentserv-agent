# Cluster Definitions

This contract defines cluster definitions, readiness, and the first local
cluster server routing boundary. A cluster is a named user-facing routing
object that owns internal route targets. A cluster server reuses the stored
server lifecycle while selecting a local model target by HTTP endpoint family.

## Scope

In scope:

- one public `cluster_ref`
- canonical TOML storage under `TENTGENT_HOME`
- CLI file apply, validation, list, inspect, run, and remove operations
- daemon REST JSON CRUD for stored definitions
- model/provider reference validation for declared routes
- read-only route readiness diagnostics during inspect and doctor checks
- model delete and capability mutation protection for stored local cluster
  routes
- local cluster server routing for `chat`, `embedding`, `rerank`,
  `audio-transcription`, and `vision-chat`
- stored server lifecycle, health, logs, and removal protection for cluster
  servers

Out of scope:

- active runtime ownership
- provider target execution through cluster servers
- route variants such as `chat.fast` and `chat.quality`
- fixed `model + adapter` cluster targets
- YAML input

## Identity And Storage

Cluster refs are exact lowercase ASCII slugs:

```text
[a-z0-9][a-z0-9._-]{0,63}
```

Slashes, whitespace, uppercase letters, hidden path segments, and
shell-sensitive characters are invalid.

`cluster_ref` is a user-chosen stable name for a mutable routing definition,
not a content hash. Model refs are hash-like because managed models are stored
and deduplicated as local assets. Clusters are configuration objects that users
edit, apply, inspect, and later route through by name. Hashing the definition
would change the route identity after ordinary edits; a generated opaque ID
would still need a user-facing alias. Reapplying the same `cluster_ref`
replaces that cluster definition. Renaming a cluster is modeled as applying a
new `cluster_ref` and removing the old one.

Stored definitions are canonical TOML:

```text
TENTGENT_HOME/
└── clusters/
    └── <cluster_ref>/
        └── cluster.toml
```

`apply` is a full replacement. A route omitted from the replacement definition
is unbound from stored state.

## Definition Shape

```toml
schema_version = 1
cluster_ref = "local-assistant"

[routes.chat]
kind = "local-model"
model_ref = "<model-ref>"

[routes.embedding]
kind = "provider"
provider = "openai"
provider_model = "<provider-model>"
```

Supported route keys:

- `chat`
- `embedding`
- `rerank`
- `audio-transcription`
- `vision-chat`

Supported target kinds:

| Kind | Fields | Meaning |
| --- | --- | --- |
| `local-model` | `model_ref`, optional `runtime_profile` | Route cluster server requests to one managed local model. `model_ref` is the full canonical model ref, not a short selector. |
| `provider` | `provider`, `provider_model` | Valid definition and readiness target. Cluster server execution returns `cluster_route_target_unsupported` until provider orchestration is added. |

`runtime_profile` uses the same stored shape as server runtime profiles:

```toml
[routes.chat.runtime_profile]
profile_id = "local-chat-mlx"
profile_version = 1
```

If `runtime_profile` is present, validation checks that it matches the known
profile for the selected route/backend tuple. If omitted, validation does not
block on profile selection.

## Validation Rules

A definition is accepted only when:

- `schema_version` is `1`
- the body `cluster_ref` matches the path `cluster_ref` for REST apply
- at least one route is declared
- every route key is supported
- every local `model_ref` exists
- every local model has the route's required capability
- every optional runtime profile matches the inferred backend
- every provider target has a non-empty `provider_model`
- the selected provider supports the route family

Partial clusters are valid. A cluster with only `chat` promises only `chat`.
Unconfigured server routes fail with `cluster_route_missing` rather than
falling back to chat or making the stored definition invalid. Running a cluster
as a server requires `routes.chat` to exist and use a local model target.

## Adapter Behavior

Cluster definitions do not store fixed `model + adapter` route targets in the
`v1.1.0` MVP. Chat routes bind the base model and route family only.

Request-time `adapter_ref` remains dynamic and is validated by the existing
server-chat adapter path. If a future route-policy feature stores adapter
allowlists or defaults, it must use the shared resource blocker model.

## Route Readiness

Cluster definitions are write-only validation objects at apply time. Readiness
is computed on demand from existing local state during:

- `tentgent cluster inspect <cluster-ref>`
- `GET /v1/clusters/{cluster_ref}`
- `tentgent doctor`
- `GET /v1/doctor`

Readiness is not written back to `cluster.toml`, and there is no persisted
readiness cache. `apply` and REST `PUT` only parse, validate, and store the
definition. A running cluster server separately caches the parsed definition
for request routing; that runtime cache does not store readiness or change the
canonical TOML.

Aggregate readiness status:

| Status | Meaning |
| --- | --- |
| `ready` | Every declared route is ready enough for the current known state. |
| `partial` | At least one route is ready and at least one declared route needs attention. |
| `blocked` | Declared routes exist, but none is currently ready. |
| `unknown` | No routes exist or readiness could not be computed. |

Per-route readiness can report:

- `ready`, `verified`, or `supported`
- `unknown`, `stale`, `failed`, or `unsupported`
- `auth-missing` or `auth-attention` for provider routes
- `unavailable` when supporting local state cannot be inspected

Local route readiness is derived from model metadata, declared capability,
runtime backend/profile selection, support hints, and existing proof records.
If a route omits `runtime_profile`, inspect may report an inferred effective
profile with the `runtime-profile-inferred` flag. The stored TOML is not
rewritten.

Provider route readiness uses non-secret auth status only. It must not resolve
or validate provider secrets, read Keychain secret material, call provider
APIs, or start a runtime. Missing or stale provider auth reports a next action
instead of blocking cluster storage.

Next-action codes are stable strings intended for CLI and REST clients:

- `inspect-cluster`
- `inspect-model`
- `verify-model-capability`
- `clear-model-proof`
- `set-provider-auth`
- `update-cluster-definition`
- `choose-supported-route-target`

## CLI Surface

```bash
tentgent cluster apply <CLUSTER_TOML>
tentgent cluster validate <CLUSTER_TOML>
tentgent cluster ls
tentgent cluster inspect <cluster-ref>
tentgent cluster run <cluster-ref> [--host <host>] [--port <port>] [--detach]
tentgent cluster rm <cluster-ref>
```

`<CLUSTER_TOML>` is an explicit file path supplied by the user. The CLI does
not support include/import directives. It rejects directories, symlinks,
special files, oversized files, and obvious secret-bearing paths unless
`--force` is passed. `--force` only bypasses the source-location warning; it
does not bypass schema or reference validation.

`cluster inspect` renders the stored definition plus read-only readiness:
summary, route table, problem flags, and next actions. It should not run model
verification, provider auth validation, or runtime startup.

`cluster run` creates or reuses a normal stored server spec with
`runtime_kind = "cluster"` and launches it. `--allow-unverified` allows
`unknown` or `stale` local support evidence for that launch; it never allows
`failed`, `unsupported`, or unavailable routes. The option is launch state and
is not persisted in `server.toml`. Use `tentgent server ls`, `server inspect`,
`server start`, `server stop`, and `server rm` for the resulting server ref.

## Cluster Server Runtime

Endpoint families select fixed cluster route keys:

| Ingress | Cluster route |
| --- | --- |
| `/v1/chat`, `/v1/chat/stream`, `/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{operation}` | `chat` |
| `/v1/embeddings` | `embedding` |
| `/v1/rerank` | `rerank` |
| `/v1/audio/transcriptions` | `audio-transcription` |
| `/v1/vision/chat` | `vision-chat` |

Provider-shaped `model` fields and Gemini path model names are compatibility
inputs only. They do not override the target stored in `cluster.toml`.
Provider-shaped request validation, response conversion, streaming, and local
runtime evidence reuse the existing local model server adapters. Unknown paths
are not forwarded to the Python runtime.

Route execution uses these error codes:

| Code | HTTP | Meaning |
| --- | --- | --- |
| `cluster_route_missing` | `400` | The endpoint family has no configured route. |
| `cluster_route_target_unsupported` | `400` | The route currently selects a provider target. |
| `cluster_route_not_ready` | `409` | Current support evidence does not allow execution. |
| `cluster_route_proof_stale` | `409` | Tuple evidence is stale and the launch did not allow it. |
| `cluster_route_proof_failed` | `409` | Current tuple evidence records a failed runtime attempt. |
| `cluster_route_unsupported` | `404` or `409` | The path or selected model tuple is unsupported. |
| `cluster_route_unavailable` | `503` | Required local model state cannot be loaded. |
| `cluster_definition_reload_failed` | `503` | The stored definition changed but could not be safely reloaded. |

The server keeps one parsed definition snapshot. Requests perform a cheap file
revision check; changed definitions are reloaded and assigned a SHA-256
definition hash. Reload failures stop request routing until the definition is
valid again. The server never silently uses the previous target after a failed
reload. `/healthz` exposes the cluster ref, current definition hash, and route
keys without loading model runtimes.

## Daemon REST Surface

```text
GET /v1/clusters
PUT /v1/clusters/{cluster_ref}
GET /v1/clusters/{cluster_ref}
DELETE /v1/clusters/{cluster_ref}
```

REST apply accepts a JSON version of the same definition structure and stores
the canonical TOML file. The daemon does not read arbitrary client-local files
for REST apply.

Cluster server specs are created through the existing server registry route:

```json
POST /v1/servers
{
  "runtime_kind": "cluster",
  "cluster_ref": "local-assistant",
  "host": "127.0.0.1",
  "port": 8780,
  "allow_unverified": false
}
```

Cluster requests must not include `runtime_ref` or a single `capability`.
Server responses retain legacy flat target fields and add a structured target:

```json
{
  "runtime_kind": "cluster",
  "cluster_ref": "local-assistant",
  "capability": null,
  "model_ref": null,
  "provider": null,
  "provider_model": null,
  "target": {
    "kind": "cluster",
    "cluster_ref": "local-assistant"
  }
}
```

Apply responses return stored definition details only:

```json
{
  "cluster": {
    "cluster_ref": "local-assistant",
    "schema_version": 1,
    "routes": [
      {
        "route": "chat",
        "kind": "local-model",
        "model_ref": "<model-ref>"
      }
    ],
    "home_dir": "/path/to/tentgent-home",
    "cluster_dir": "/path/to/tentgent-home/clusters/local-assistant",
    "definition_path": "/path/to/tentgent-home/clusters/local-assistant/cluster.toml"
  }
}
```

Inspect responses add read-only readiness fields:

```json
{
  "cluster": {
    "cluster_ref": "local-assistant",
    "schema_version": 1,
    "readiness": {
      "status": "partial",
      "ready_route_count": 1,
      "attention_route_count": 1,
      "flags": ["partial-cluster"]
    },
    "routes": [
      {
        "route": "chat",
        "kind": "local-model",
        "capability": "chat",
        "model_ref": "<model-ref>",
        "runtime_profile_readiness": {
          "effective": {
            "profile_id": "local-chat-transformers-peft",
            "profile_version": 1
          },
          "source": "inferred"
        },
        "backend": "safetensors",
        "readiness": {
          "status": "verified",
          "description": "latest local proof verified chat"
        },
        "next_actions": []
      }
    ],
    "home_dir": "/path/to/tentgent-home",
    "cluster_dir": "/path/to/tentgent-home/clusters/local-assistant",
    "definition_path": "/path/to/tentgent-home/clusters/local-assistant/cluster.toml"
  }
}
```

The route-level `runtime_profile` field is a legacy configured-profile field
retained for compatibility. New clients should use
`runtime_profile_readiness`, which includes configured, effective, and source
details.

List responses return compact route keys:

```json
{
  "clusters": [
    {
      "cluster_ref": "local-assistant",
      "routes": ["chat", "embedding"]
    }
  ]
}
```

Remove responses return pre-removal metadata:

```json
{
  "removed": {
    "kind": "cluster",
    "cluster_ref": "local-assistant",
    "cluster_dir": "/path/to/tentgent-home/clusters/local-assistant"
  },
  "cluster": {
    "...": "same shape as GET /v1/clusters/{cluster_ref}"
  }
}
```

## Resource Protection

Stored local cluster routes are model bindings. A model referenced by a cluster
route cannot be deleted normally, and a model capability used by a stored route
cannot be removed from that model's capability metadata.

The current implementation reports these blockers in the existing model-store
error path. The shared structured blocker shape is defined in
[resource-blockers.md](./resource-blockers.md).

Cluster removal is also blocked while any running or stopped server spec
targets that cluster. Stop a running server, remove its stored server spec, and
then remove the cluster. `cluster rm` never cascades into server specs, models,
adapters, or runtime profiles. REST reports this conflict as `cluster_in_use`.
