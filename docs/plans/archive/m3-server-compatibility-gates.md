# M3 Server Compatibility Gates

This is the focused execution plan for M3 in the
[capability-first release roadmap](./capability-first-release-roadmap.md).

Status: implemented in the current working tree.

## Goal

- Persist and expose the endpoint family a server spec is meant to serve.
- Use M1/M2 model capability metadata as the source of truth for endpoint
  compatibility.
- Reject incompatible local server starts and chat-family requests before a
  Python runtime or model backend is invoked.
- Keep chat sessions and transcript storage isolated from embedding and rerank
  work.

## Non-Goals

- Do not implement `POST /v1/embeddings`; that remains M4.
- Do not implement `POST /v1/rerank`; that remains M5.
- Do not add embedding or rerank backend execution paths.
- Do not add audio capability names or routes.
- Do not infer backend readiness from `model_capabilities`.

## Starting Baseline

- Model metadata already records `model_capabilities` and
  `model_capability_source`.
- Old model metadata without `model_capabilities` is read as `["chat"]`.
- Chat model resolution rejects local models that do not advertise `chat`.
- Local server preparation and launch validation already reject non-chat models
  for the current chat-only server path.
- Server specs and daemon server DTOs do not yet expose a server capability.

## Compatibility Matrix

| Endpoint family | Required server capability | Required model capability | M3 behavior |
| --- | --- | --- | --- |
| Chat routes | `chat` | `chat` | Implement and test rejection before runtime dispatch. |
| Embeddings | `embedding` | `embedding` | Define the gate shape only; route implementation waits for M4. |
| Rerank | `rerank` | `rerank` | Define the gate shape only; route implementation waits for M5. |

Chat routes include native `/v1/chat`, OpenAI-compatible
`/v1/chat/completions`, Claude-compatible `/v1/messages`, and Gemini-compatible
text generation routes. All of them should reach the same model capability
check through the chat use-case boundary.

## Execution Slices

### 1. Contract And Domain Surface

- Add a single server endpoint capability value to local server specs. Use the
  same vocabulary as model capabilities: `chat`, `embedding`, and `rerank`.
- Default missing server capability in stored specs to `chat` so existing
  `server.toml` files stay readable.
- Keep each server spec single-capability. Multi-capability serving can be
  revisited after M4/M5 prove the endpoint contracts.
- Keep cloud provider server specs chat-only in M3.
- Avoid changing existing chat server identity behavior in this slice. If a
  future slice enables non-chat server spec creation, review whether capability
  should enter the server identity hash at that point.

### 2. Kernel Compatibility Helpers

- Add a small server-domain compatibility helper that checks:
  - selected server capability
  - required model capability
  - advertised model capabilities
- Return `KernelError::UnsupportedTarget` with a message that names the server
  capability, required model capability, model ref, and advertised
  capabilities.
- Use the helper from local server preparation and `resolve_for_start`.
- Add regression coverage for:
  - old specs without capability defaulting to `chat`
  - chat server with chat model passing
  - chat server with embedding model rejected
  - chat server with rerank model rejected
  - stored non-chat server specs not launchable until their endpoints exist

### 3. Daemon Server DTOs

- Include server capability in server list, create, inspect, start, stop, and
  remove response DTOs.
- Keep the response field name stable and explicit, preferably `capability`.
- Ensure JSON snapshots or route tests cover the new field for old and new
  specs.
- Update user-facing command examples only if existing server output examples
  show server DTO fields.

### 4. Chat Request Gates

- Keep chat-family request compatibility centralized in the chat model resolver.
- Add daemon route tests proving embedding and rerank models are rejected by:
  - native `/v1/chat`
  - OpenAI-compatible `/v1/chat/completions`
  - Claude-compatible `/v1/messages`
  - Gemini-compatible text generation routes
- Ensure the failure is a clear client error, not a Python runtime failure.
- Preserve existing session behavior: session-backed chat continues to use chat
  use cases and transcript stores only for chat.

### 5. Future Endpoint Gate Shape

- Add or document a reusable endpoint-family check that M4 and M5 must call
  before dispatching embedding or rerank runtime work.
- Do not add public embedding or rerank routes in M3 unless they return a clear
  not-implemented response and are not documented as available.
- Keep embedding and rerank session/transcript storage out of scope.

### 6. Documentation

- Update `docs/contracts/tentgent-daemon.md` if daemon server DTO fields change.
- Update `docs/contracts/http-daemon.md` if REST server response shapes change.
- Update `docs/user/commands.md` or `docs/user/version.md` only when user-visible
  output or limitations change.
- Keep `docs/contracts/model-store.md` unchanged unless the meaning of
  `model_capabilities` changes.

## Verification

Run the focused checks first:

```bash
cargo test -p tentgent-kernel server
cargo test -p tentgent-kernel chat
cargo test -p tentgent-daemon server
cargo test -p tentgent-daemon chat
```

Run the workspace check before review:

```bash
cargo check --workspace
```

## Completion Criteria

- Existing server specs without a capability field load as `chat`.
- Daemon server DTOs expose server capability.
- Local server prepare/start cannot launch an embedding or rerank model through
  the chat server family.
- All chat-compatible daemon routes reject embedding and rerank models before
  runtime dispatch.
- No public docs imply that embedding or rerank runtime endpoints are available
  before M4/M5.
