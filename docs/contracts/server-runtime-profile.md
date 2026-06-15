# Server Runtime Profile Contract

This contract defines runtime profile metadata attached to model-bound server
starts.

## Scope

Runtime profiles describe how a local server capability maps to a backend
family. They are selected by the kernel when `tentgent server run <model-ref>`
creates a local server spec.

This is separate from Python dependency bootstrap profiles such as
`tentgent runtime bootstrap --profile local-model`. Bootstrap profiles install
packages. Server runtime profiles describe the selected server execution path.

## Current Selection

The first slice covers local chat servers only.

| Capability | Backend | Profile |
| --- | --- | --- |
| `chat` | `transformers-peft` | `local-chat-transformers-peft-v1` |
| `chat` | `mlx` | `local-chat-mlx-v1` |
| `chat` | `llama-cpp` | `local-chat-llama-cpp-v1` |

`embedding`, `rerank`, media, and cloud provider server profiles are later
runtime-profile slices.

## Stored Metadata

Local server specs may include:

```toml
[runtime_profile]
profile_id = "local-chat-mlx"
profile_version = 1
```

The display label is `<profile_id>-v<profile_version>`, such as
`local-chat-mlx-v1`.

Existing server specs without `runtime_profile` remain readable. New local chat
server specs should include the selected runtime profile.

## Identity And Launch

For local model-bound servers, the selected runtime profile participates in the
server identity. If a future version selects a different profile for the same
model, Tentgent should create a new server spec instead of silently reusing an
old one.

The launcher passes the selected profile to the hidden local server runtime:

```bash
__local-server-runtime ... --runtime-profile local-chat-mlx-v1
```

The local server runtime exposes the value in `/healthz` as
`runtime_profile`.

## Parameter Metadata

Runtime profile records may list accepted and rejected request parameters and
safe default limits. In this slice, local chat profiles record the recognized
chat parameter boundary but do not enforce new limits beyond existing request
validation.

Future slices may use runtime profiles to decide backend-specific defaults,
hard limits, verification stale rules, and server-start gating.

## Operator Visibility

`tentgent server inspect <server-ref>` shows `runtime_profile` and
`runtime_profile_version` for local specs that include profile metadata.
Cloud provider servers do not show local runtime profile metadata in this
contract.
