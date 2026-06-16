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

The current local runtime profile slices cover local chat and local embedding
servers.

| Capability | Backend | Profile |
| --- | --- | --- |
| `chat` | `transformers-peft` | `local-chat-transformers-peft-v1` |
| `chat` | `mlx` | `local-chat-mlx-v1` |
| `chat` | `llama-cpp` | `local-chat-llama-cpp-v1` |
| `embedding` | `transformers-peft` | `local-embedding-transformers-peft-v1` |
| `embedding` | `llama-cpp` | `local-embedding-llama-cpp-v1` |

`embedding` on the `mlx` backend currently has no local runtime profile because
the bundled Apache-licensed runtime recognizes the path but returns
`501 not_implemented`. `rerank`, media, and cloud provider server profiles are
later runtime-profile slices.

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
and embedding server specs should include the selected runtime profile. Local
chat and embedding specs missing a required runtime profile should fail before
runtime launch instead of silently dispatching to an ambiguous backend path.

## Identity And Launch

For local model-bound servers, the selected runtime profile participates in the
server identity. If a future version selects a different profile for the same
model, Tentgent should create a new server spec instead of silently reusing an
old one.

The launcher passes the selected profile to the hidden local server runtime:

```bash
__local-server-runtime ... --runtime-profile local-embedding-transformers-peft-v1
```

The local server runtime exposes the value in `/healthz` as
`runtime_profile`.

## Parameter Metadata

Runtime profile records may list accepted and rejected request parameters and
safe default limits.

Local chat profiles currently record `messages`, `temperature`, `max_tokens`,
and `stream` as recognized request parameters. Unsupported provider-style chat
fields such as `tools`, `tool_choice`, `response_format`, `audio`, and
`modalities` remain rejected by existing request validation.

Local embedding profiles currently record `input`, `model`, and
`encoding_format=float` as recognized OpenAI-compatible ingress parameters.
`dimensions`, `encoding_format=base64`, `user`, and unknown provider fields are
rejected. Output vector dimensions are selected by the bound model/runtime; a
caller-supplied dimension override is not supported.

Embedding batching accepts one string or a non-empty string array. The response
preserves input order.

Future slices may use runtime profiles to decide backend-specific defaults,
hard limits, verification stale rules, and server-start gating.

## Operator Visibility

`tentgent server inspect <server-ref>` shows `runtime_profile` and
`runtime_profile_version` for local specs that include profile metadata.
Cloud provider servers do not show local runtime profile metadata in this
contract.
