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
- model delete and capability mutation protection for stored local cluster
  routes

Out of scope:

- cluster request routing
- cluster start/stop lifecycle
- active runtime ownership
- route readiness diagnostics
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

Apply and inspect responses return:

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
