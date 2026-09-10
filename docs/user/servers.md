# Model And Cloud Servers

Create a persistent HTTP entry point for one local model or cloud provider model. The daemon is optional for CLI server management. Use [Clusters](./clusters.md) for multiple capability routes on one port.

## Examples And Common Operations

Launch a stable local server proxy:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load --detach
tentgent server inspect <server-ref>
```

Local and Cluster model runtimes use separate idle controls:

```bash
tentgent server run <model-ref> \
  --runtime-idle-seconds 300 \
  --model-idle-seconds 0 \
  --lazy-load
```

`--model-idle-seconds 0` releases the model immediately after the final
request lease. The Rust proxy stays available; `--runtime-idle-seconds 300`
lets the subordinate Python process exit after five minutes without accepted
work, and a later request starts it again. The model value must be less than or
equal to the runtime value. `--idle-seconds` remains a deprecated alias for
`--runtime-idle-seconds`; if both are supplied they must match.

`--port` is optional. When omitted, Tentgent creates an auto-port server spec
that starts scanning at `8780` each time the server is launched. The first free
port is recorded as the running process `bound_port`; `server ls`, `server ps`,
and daemon health calls use that actual port. When `--port` is provided, that
port is fixed and startup fails if it is unavailable.
`server ls` keeps local model rows compact by showing the model `short_ref` in
the `target` column. Cloud rows show the provider model name and cluster rows
show the `cluster_ref`. Use `server inspect <server-ref>` when full target
details are needed.

Local model-bound server creation checks the selected capability, runtime
profile availability, and effective support status before saving or launching
the server. `verified` local proofs and `supported` catalog hints are allowed
by default. `failed` and `unsupported` are blocked. `unknown` and `stale` are
blocked unless you explicitly retry with `--allow-unverified`:

```bash
tentgent server run <model-ref> --capability chat --allow-unverified
tentgent server start <server-ref> --allow-unverified
```

When `--capability` is omitted for a local model, Tentgent chooses the server
endpoint family from the model's stored capabilities. The priority is
`video-understanding`, `vision-chat`, `image-generation`, `audio-transcription`,
`audio-speech`, `rerank`, `embedding`, then `chat`. Use `--capability chat`,
`embedding`, `rerank`, `audio-transcription`, `audio-speech`, `vision-chat`,
`video-understanding`, or `image-generation` to override that choice. Local
servers bind the selected model in their Rust proxy spec, so the direct server
request body does not need `model_ref`, `model`, or `model_kind` fields. The
proxy starts or reuses the shared Python model runtime on demand; that Python
runtime may idle-shutdown and be started again on a later request. Health and
inspect operations do not extend either idle clock. Direct Python runtime
callers that do not start a model-bound server may still send explicit `model`
and `model_kind` fields.

For local model-bound servers, `server inspect` includes a `model_support` row
for the server capability selected at creation time. This row reports the
current local proof or catalog-derived support status for the bound model,
selected runtime profile, runtime profile version, execution backend, and
copyable next action for failed, stale, unknown, or unsupported tuples. Local
chat servers also show `runtime_profile` and `runtime_profile_version` when the
server spec records the selected backend profile, such as `local-chat-mlx-v1`.
Runtime profiles are server execution metadata, not dependency bootstrap
profiles; see [server-runtime-profile.md](../contracts/server-runtime-profile.md).
Cloud provider servers do not show local model support because they are bound
to provider-hosted models rather than records in the local model store.

`server inspect` also shows a safe runtime ownership summary. It does not expose
process-instance tokens, lock paths, or internal request lease ids.
For Local and Cluster targets it also shows the concrete effective
`runtime_idle_seconds` and `model_idle_seconds`, including the `300` / `0`
defaults when the stored spec omitted explicit overrides.

Use `doctor` when you want the same support diagnostics across all stored local
models and stored clusters. `doctor` keeps the main check list compact and
places long profile, backend, failure, and next-action details in the `Details`
block.
`doctor` also warns when a stored local model is missing files that would block
runtime execution and points back to `tentgent model inspect <model-ref>` for
the exact missing path and recovery action. Local model-bound server starts run
the same blocking file preflight before launching the runtime; `--allow-unverified`
only bypasses unknown or stale support evidence, not missing required files.

Call the server:

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Hello there"}
    ],
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

Direct model-server chat and daemon chat are stateless. Use CLI `chat --session`
for automatic context and transcript recording, or explicitly manage messages
through the [session API](./sessions.md#http-api). Neither chat endpoint accepts
`session_ref` or `max_session_messages`.

For OpenAI, Claude, and Gemini request formats and SDK setup, see [provider integrations](./provider-compatible-examples.md). For direct embedding/rerank requests, see [Embeddings and reranking](./inference/embedding-rerank.md).

### Background Lifecycle

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --detach
tentgent server ls
tentgent server ps
tentgent server inspect <server-ref>
tentgent server stop <server-ref>
tentgent server start <server-ref>
tentgent server stop <server-ref>
tentgent server rm <server-ref>
```

`run` creates or reuses a stored spec and launches it. `start` launches an existing spec in the background. `stop` leaves the spec available for reuse; `rm` removes a stopped spec.

### Cloud Servers

Configure [provider credentials](./auth.md) first. Replace the provider model name with one available to your account:

```bash
tentgent server run openai:<provider-model> --port 8783 --detach
tentgent server run anthropic:<provider-model> --port 8784 --detach
tentgent server run gemini:<provider-model> --capability embedding --port 8785 --detach
```

Cloud servers use provider keys from
env/keychain at launch and expose `/v1/chat`, `/v1/chat/completions`,
`/v1/messages`, `/v1/embeddings`, and `/v1/images/generations` when the bound
provider supports that endpoint family. Explicit cloud server capabilities are
accepted for `chat`, `vision-chat`, `embedding`, and `image-generation`;
unsupported provider combinations are rejected at server spec creation.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `server run` | `-H, --home <HOME>` | Optional Tentgent runtime home override for server state and model lookup |
| `server run` | `-a, --host <HOST>` | Interface for the active HTTP listener; use 127.0.0.1 for loopback. |
| `server run` | `-p, --port <PORT>` | Fixed TCP port for the HTTP listener. Omit to auto-scan from 8780 |
| `server run` | `-l, --lazy-load` | Stored preference; current local proxies load on demand even when it is omitted. |
| `server run` | `-i, --idle-seconds <N>` | Deprecated alias for --runtime-idle-seconds |
| `server run` | `--runtime-idle-seconds <N>` | Shut down the managed Python runtime after N workload-idle seconds |
| `server run` | `--model-idle-seconds <N>` | Release the loaded model after N model-idle seconds. Defaults to 0 |
| `server run` | `--capability <CAPABILITY>` | Endpoint family to serve from the selected runtime |
| `server run/start` | `--allow-unverified` | Allow local server start when model support status is unknown or stale |
| `server run` | `-d, --detach` | Launch the initial server process in background mode and return immediately |
| `server ls/ps/inspect/start/stop/rm` | `-H, --home <HOME>` | Optional Tentgent runtime home override for server state lookup |
| `server start` | `-d, --details` | Show the full inspection table after the server starts |
| `server stop` | `-d, --details` | Show the full inspection table after the server stops |
| `server rm` | `-d, --details` | Show the full inspection table captured before the server is removed |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/servers` | None. |
| `POST` | `/v1/servers` | Target and listener settings; see creation fields below. |
| `GET` | `/v1/servers/{reference}` | None. |
| `DELETE` | `/v1/servers/{reference}` | Removes a stopped server spec. |
| `POST` | `/v1/servers/{reference}/start` | `{"wait_ready":true,"timeout_seconds":30,"allow_unverified":false}` |
| `POST` | `/v1/servers/{reference}/stop` | None. |
| `GET` | `/v1/servers/{reference}/health` | Probe server process health. |
| `GET` | `/v1/servers/{reference}/logs` | Server log metadata. |
| `GET` | `/v1/servers/{reference}/logs/stdout?tail_bytes=8192` | Server stdout tail. |
| `GET` | `/v1/servers/{reference}/logs/stderr?tail_bytes=8192` | Server stderr tail. |

Direct model-server ports are separate from the daemon port. A server exposes
only the endpoint family selected by its `capability`, such as `/v1/chat`,
`/v1/embeddings`, `/v1/rerank`, audio, vision, video, or image routes.
Cluster server specs have no single capability. Their endpoint family selects
the matching stored cluster route, and caller `model` fields cannot replace the
configured target. Cluster creation requires `runtime_kind: "cluster"` and
`cluster_ref`; it rejects mixed `runtime_ref` or `capability` fields.
Omitting `port` creates an auto-port server spec that starts scanning at `8780`
on every launch. Explicit `port` values are fixed. Server responses expose
`requested_port`, `port_auto`, and the running process `bound_port`; the top-level
`port` is the effective port clients should call. List, inspect, and create
responses also expose a structured `target`; cluster targets use
`{"kind":"cluster","cluster_ref":"<cluster-ref>"}` while existing flat fields
remain present.
Unsupported endpoint families on that direct server should return `404` or an
endpoint-specific error.

`runtime_idle_seconds` and `model_idle_seconds` apply to Local and Cluster
model runtimes and default to `300` and `0`. Both are non-negative and the
model value cannot exceed the runtime value. Deprecated `idle_seconds` is an
input and response mirror for runtime idle only; supplying it together with a
different canonical value returns `400`. List and create responses return the
canonical optional fields plus the legacy mirror. Detailed inspection also
returns `effective_runtime_idle_seconds` and
`effective_model_idle_seconds`. Health and inspection do not refresh either
idle clock.

### Creation Fields

| Field | Type and rule |
| --- | --- |
| `runtime_ref` | Local model ref or `provider:model` string for local/cloud targets. |
| `runtime_kind`, `cluster_ref` | Use `"cluster"` plus a stored Cluster ref for Cluster targets; omit `runtime_ref` and `capability`. |
| `capability` | Optional local/cloud endpoint family; CLI values are listed above. |
| `host`, `port` | Optional interface string and integer port; omitted port enables auto-selection. |
| `lazy_load` | Optional boolean preference; see lifecycle limits below. |
| `runtime_idle_seconds`, `model_idle_seconds` | Optional non-negative integer timeouts for Local/Cluster runtimes. |
| `idle_seconds` | Deprecated runtime timeout alias; it must match the canonical field when both are present. |
| `allow_unverified` | Optional boolean launch admission override; only unknown/stale evidence can be bypassed. |

### Create, Start, And Inspect Over HTTP

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/servers \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"runtime_ref":"<model-ref>","capability":"chat","host":"127.0.0.1","port":8780,"runtime_idle_seconds":300,"model_idle_seconds":30}'
curl -sS -X POST 'http://127.0.0.1:8790/v1/servers/<server-ref>/start' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"wait_ready":true,"timeout_seconds":30}'
curl -sS 'http://127.0.0.1:8790/v1/servers/<server-ref>/health' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

HTTP create stores the spec and returns `server` plus `created`; call `/start`
separately. Read `server.server_ref` from that response. Start returns `server`
and optional `readiness`; inspection returns `server`, and list returns
`servers`. Use `server.port` as the effective client port and `running` for
process state. Health also reports `reachable`, `target_status`, and `error`.
Logs and absolute paths are daemon-host diagnostics.

### Current Lifecycle Limits

Local and Cluster proxies currently load on demand regardless of the stored
`lazy_load` preference. Cloud targets also accept lifecycle settings that do
not control a local Python model/runtime. Do not rely on those settings for
cloud retention or shutdown; [issue #132](https://github.com/HiroLiang/tentserv-agent/issues/132)
tracks the behavior. The `runtime_idle_seconds` / `model_idle_seconds` rules
above describe Local and Cluster runtimes.

## Related Guides

[Chat](./inference/chat.md) · [Listener addresses](./daemon.md#http-listener-addresses) · [Clusters](./clusters.md) · [Provider integrations](./provider-compatible-examples.md)
