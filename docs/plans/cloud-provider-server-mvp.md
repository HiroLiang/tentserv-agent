# Cloud Provider Server MVP

This plan defines the next active track: let `tentgent server` expose OpenAI and Claude through the same local HTTP chat surface used by local model servers.

## Priority

Do this before provider-backed dataset synthesis. The provider client, auth preflight, and response normalization can then be reused by `dataset synth` and `dataset eval`.

## Scope

- Add OpenAI and Anthropic as cloud runtime providers for `tentgent server`.
- Reuse `auth openai` and `auth anthropic` for secret resolution.
- Keep provider secrets out of persisted server specs.
- Reuse the existing non-streaming `POST /v1/chat` request shape.
- Keep local managed models separate from cloud provider model names.

## Non-Goals

- Do not add OpenAI or Claude to the model store.
- Do not support LoRA adapters for cloud provider servers.
- Do not implement cloud dataset generation in this track.
- Do not implement full OpenAI-compatible API routing.
- Do not implement streaming in the first pass.

## Command Shape

Preferred first syntax:

```text
tentgent server run openai:<MODEL_NAME> --host 127.0.0.1 --port 8780
tentgent server run anthropic:<MODEL_NAME> --host 127.0.0.1 --port 8781
```

`claude:<MODEL_NAME>` is accepted as an alias for `anthropic:<MODEL_NAME>` and stores the provider as `anthropic`.

Keep existing local model syntax unchanged:

```text
tentgent server run <MODEL_REF>
```

Cloud runtime refs are not model refs. They are server runtime refs that identify a provider and provider model.

## Runtime Spec

Extend stored server specs so one spec can describe either a local model or a cloud provider runtime.

First-pass shape:

```text
runtime_kind = "local" | "cloud"
model_ref = "<local-model-ref>"              # local only
provider = "openai" | "anthropic"            # cloud only
provider_model = "<provider-model-name>"     # cloud only
```

Rules:

- do not store API keys in `server.toml`
- resolve secrets at server launch or start time
- pass the selected secret to the runtime process as an environment variable only
- fail before launching when the effective provider key is missing or invalid

## Execution Order

### Slice 1: Runtime Ref And Spec Contract

Lock the cloud runtime reference syntax and persisted spec shape.

Status: implemented in the active workspace.

Goals:

- parse `openai:<MODEL_NAME>`, `anthropic:<MODEL_NAME>`, and the `claude:<MODEL_NAME>` alias
- keep existing `<MODEL_REF>` behavior unchanged
- add cloud runtime fields to server spec serialization
- update server list and inspect rendering to show local vs cloud runtimes
- avoid launching cloud runtime code yet

Review target:

- users can create and inspect a cloud server spec without secrets or network calls

### Slice 2: Auth Preflight And Secret Injection

Wire provider auth into server launch.

Status: implemented in the active workspace for auth preflight. Environment handoff is constrained to the future cloud runtime process launch because Slice 1 still blocks before starting cloud runtime code.

Goals:

- resolve effective provider secret through existing auth rules
- validate the provider key before launch
- pass the secret to the Python runtime environment
- ensure persisted specs, logs, and tables never print the secret
- make `server start` re-resolve the current key instead of reusing stale secret state

Review target:

- cloud server launch fails early with clear auth errors when keys are missing, invalid, or unknown

### Slice 3: Provider Chat Client Boundary

Add a small provider client layer in the Python daemon.

Goals:

- support OpenAI and Anthropic non-streaming chat calls
- accept normalized Tentgent messages: `system`, `user`, and `assistant`
- map `max_tokens` and `temperature` where the provider supports them
- return normalized generated text
- keep provider request/response parsing outside server HTTP handlers

Review target:

- provider clients can be tested with mocked HTTP responses without starting a Tentgent server

### Slice 4: Cloud Runtime Session

Teach the Python server runtime to handle cloud specs.

Goals:

- bypass local model loading for cloud runtimes
- keep `GET /healthz` meaningful for cloud servers
- use provider client generation for `POST /v1/chat`
- return `501` for adapter requests against cloud runtimes
- return `501 stream_not_implemented` for `stream=true`

Review target:

- `tentgent server run openai:<MODEL>` can serve one non-streaming `/v1/chat` request

### Slice 5: CLI And Server Lifecycle Polish

Make cloud servers feel consistent with local servers.

Goals:

- support foreground and `--detach` launch modes
- support `server ls`, `ps`, `inspect`, `start`, `stop`, and `rm`
- show provider/model in tables without pretending it is a local `model_ref`
- add concise errors for unsupported adapter and local-model-only options

Review target:

- cloud and local server specs can coexist and be managed with the same commands

### Slice 6: Smoke Tests And Docs

Add reviewable verification around the new path.

Goals:

- add mocked provider-client tests
- add command help snapshots or focused CLI tests where practical
- document auth prerequisites and example curl calls
- document that cloud servers are proxy runtimes, not managed local models

Review target:

- contributors can validate the feature without spending provider credits in default tests

### Slice 7: Streaming Follow-Up

Defer streaming until the non-streaming path is stable.

Goals:

- define provider streaming normalization
- preserve the existing `stream=true` request shape
- avoid provider-specific streaming leaks in the local HTTP contract

Review target:

- streaming can be reviewed independently from core provider routing

## Open Questions

- Should provider model names have defaults, or should the user always pass one explicitly?
- Should `auth status` be required to show `verified` before launch, or should `unknown` be allowed with a warning and explicit flag?
