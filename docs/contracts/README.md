# Contracts

Use this directory for concise interface documents that define stable boundaries between repository components.

## Scope

- Describe contracts between Rust entry points and the Python daemon.
- Describe backend routing rules, adapter lifecycle rules, and request or response shapes when they become stable enough to document.
- Describe runtime-home conventions, environment-variable overrides, and stable storage boundaries.
- Describe provider auth storage and resolution rules when secrets cross process boundaries.
- Describe model-store identity, deduplication, and import or pull boundaries when model management behavior changes.
- Describe adapter-store identity, compatibility, and source-index boundaries when adapter management behavior changes.
- Describe canonical dataset schemas when training, evaluation, or cloud dataset generation behavior changes.
- Describe platform and backend support boundaries before runtime routing behavior depends on them.
- Describe kernel architecture ownership and dependency direction for shared
  behavior.
- Keep each document focused on one interface or one boundary.

## Contract Index

- [auth-secrets.md](./auth-secrets.md)
  Provider-secret resolution and keychain usage rules.
- [model-store.md](./model-store.md)
  Model identity, deduplication, managed layout, and Hugging Face pull boundaries.
- [model-support-status.md](./model-support-status.md)
  Support status vocabulary, evidence precedence, stale-proof rules, and
  transition rules for model/capability/backend tuples.
- [model-support-proof-schema.md](./model-support-proof-schema.md)
  Local proof and support hint record schema for explaining model support
  status.
- [adapter-store.md](./adapter-store.md)
  Adapter identity, compatibility metadata, managed layout, and source-index draft.
- [dataset-store.md](./dataset-store.md)
  Dataset identity, managed layout, local import, and deduplication boundary.
- [dataset-schema.md](./dataset-schema.md)
  Canonical chat, tool-call, and cloud-generated dataset record schema.
- [session-store.md](./session-store.md)
  Local session metadata and transcript message store boundary.
- [server-chat.md](./server-chat.md)
  HTTP chat request shape, adapter validation rules, and runtime error mapping.
- [server-embedding.md](./server-embedding.md)
  Direct local model-server embedding request shape and capability routing.
- [server-rerank.md](./server-rerank.md)
  Direct local model-server rerank request shape and capability routing.
- [job-workspace.md](./job-workspace.md)
  Kernel-owned job workspace, chunk IO, result file, and cleanup port boundary.
- [http-daemon.md](./http-daemon.md)
  Rust HTTP daemon health/status endpoint, JSON response, and error-shape contract.
- [provider-api-errors.md](./provider-api-errors.md)
  Stable unsupported-field, content, operation, and capability error semantics
  for provider-shaped API routes.
- [tentgent-daemon.md](./tentgent-daemon.md)
  Rust daemon application host, bootstrap, transport, and runtime-state boundary.
- [training-lora.md](./training-lora.md)
  Managed LoRA train-plan identity, config shape, backend rules, and future run boundaries.
- [runtime-home.md](./runtime-home.md)
  Runtime-home resolution, standard subdirectories, and environment-variable overrides.
- [platform-backends.md](./platform-backends.md)
  Platform capability matrix and backend support guardrails.
- [kernel-architecture.md](./kernel-architecture.md)
  `tentgent-kernel` module placement, dependency direction, capability readiness, and persistence boundaries.

## Expansion Rules

- If this directory grows, split by subsystem instead of collecting unrelated notes in one file.
- Add a subfolder plus its own `README.md` when one contract area becomes too large to scan quickly.
- Keep documents concise and update them in the same change that modifies the corresponding boundary.
