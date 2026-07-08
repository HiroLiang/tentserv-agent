# Cluster Definitions

This contract defines the first cluster definition boundary. A cluster is a
named user-facing routing object that owns internal route targets. This slice
stores and validates definitions only; it does not start a multi-route runtime
or route inference requests through a cluster.

## Scope

In scope:

- one public `cluster_ref`
- canonical TOML storage under `TENTGENT_HOME`
- CLI file apply, validation, list, inspect, and remove operations
- daemon REST JSON CRUD for stored definitions
- model/provider reference validation for declared routes
- read-only route readiness diagnostics during inspect and doctor checks
- model delete and capability mutation protection for stored local cluster
  routes

Out of scope:

- cluster request routing
- cluster start/stop lifecycle
- active runtime ownership
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
| `local-model` | `model_ref`, optional `runtime_profile` | Route requests to one managed local model in a later routing slice. `model_ref` is the full canonical model ref, not a short selector. |
| `provider` | `provider`, `provider_model` | Route requests to a supported cloud provider in a later routing slice. |

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
Unconfigured routes should later fail with missing-route errors rather than
making the whole cluster invalid.

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

Readiness is not written back to `cluster.toml`, and there is no readiness
cache. `apply` and REST `PUT` only parse, validate, and store the definition.

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
