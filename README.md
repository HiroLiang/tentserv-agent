# Tentgent

Tentgent is a local AI workflow operator: a Rust CLI and HTTP daemon for
models, inference, datasets, adapters, LoRA training, and application servers.
Manage local resources, run a request, or expose model capabilities through
one tool. Cloud workflows use your configured provider credentials.

[User guide](./docs/user/README.md) · [Command index](./docs/user/commands.md) ·
[HTTP API](./docs/user/api.md) · [繁體中文](./docs/i18n/zh-TW/README.md) ·
[日本語](./docs/i18n/ja/README.md)

## Quick Start

On macOS, install from the Homebrew tap:

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent --version
tentgent runtime bootstrap
tentgent doctor
```

For Windows, Linux preview, pinned versions, upgrades, or uninstalling, use the
[installation guide](./docs/user/install.md). Backend availability varies by
platform; check [runtime support](./docs/user/runtime.md#backend-status).

Prepare local model dependencies and try a small public chat fixture:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull HuggingFaceTB/SmolLM-135M-Instruct
tentgent model ls
tentgent chat <model-ref> --message "user:Hello" --max-tokens 64
```

Replace `<model-ref>` with the ref printed by pull/list. This small fixture is
useful for checking the workflow; choose a model appropriate to your task from
the [fixture guide](./docs/user/model-fixtures.md) or
[model catalog](./docs/user/model-support-catalog.md).

Configure [provider authentication](./docs/user/auth.md) only when needed.
For example, `tentgent auth hf set` stores a Hugging Face token for a gated
repository after its publisher has granted your account access.

## Choose An Entry Point

| Entry point | Use it for |
| --- | --- |
| CLI | Resource management and foreground inference on your machine. |
| Daemon | HTTP management APIs and asynchronous workflows, normally on port 8790. |
| Model or Cluster server | Application inference on a separate port, normally starting at 8780. |

## Find A Feature

Choose what you want to do. Each linked guide includes examples and parameters;
features with HTTP endpoints also document request and response formats.

## Install, Configure, And Diagnose

| I want to… | CLI | Guide |
| --- | --- | --- |
| Install, upgrade, or select a version | `--version` | [Installation](./docs/user/install.md) |
| Prepare Python dependencies and select a backend | `runtime bootstrap/status` | [Runtime](./docs/user/runtime.md) |
| Set keys or switch env/file/Keychain modes | `auth` | [Authentication](./docs/user/auth.md) |
| Diagnose, repair stale ownership, or clean staging | `doctor`, `runtime reconcile`, `store gc` | [Maintenance](./docs/user/maintenance.md) |

## Manage Models, Adapters, And Data

| I want to… | CLI | Guide |
| --- | --- | --- |
| Pull/import a model, inspect capabilities and proof | `model` | [Models](./docs/user/models.md) |
| Import, bind, inspect, or remove an adapter | `adapter` | [Adapters](./docs/user/adapters.md) |
| Generate, validate, import, evaluate, or export data | `dataset` | [Datasets](./docs/user/datasets.md) |
| Find a small test model or supported model family | `model catalog` | [Fixtures](./docs/user/model-fixtures.md), [support catalog](./docs/user/model-support-catalog.md) |

## Run Inference

| I want to… | CLI | Guide |
| --- | --- | --- |
| Chat with a base model or adapter, optionally stream | `chat` | [Text chat](./docs/user/inference/chat.md) |
| Embed text or rank documents | `embed`, `rerank` | [Embeddings and reranking](./docs/user/inference/embedding-rerank.md) |
| Transcribe audio or generate speech | `transcribe`, `speak` | [Audio](./docs/user/inference/audio.md) |
| Ask a question about an image | `vision chat` | [Vision](./docs/user/inference/vision.md) |
| Ask a question about a video | `video understand` | [Video](./docs/user/inference/video.md) |
| Create an image from text | `image generate` | [Image generation](./docs/user/inference/images/generate.md) |
| Restyle, inpaint, or use ControlNet | `image transform/inpaint/control` | [Image editing](./docs/user/inference/images/edit.md) |

The [inference index](./docs/user/inference/README.md) explains file inputs, output paths,
multipart uploads, and which HTTP operations produce jobs.

## Train And Use LoRA

Prepare a [dataset](./docs/user/datasets.md), create and inspect a
[train plan](./docs/user/training-lora.md#plan-commands), then
[run training and select the adapter](./docs/user/training-lora.md#run-and-select-the-adapter).
The guide covers every `train lora` command, backend-specific parameters,
HTTP fields, logs, and current interruption/resume limits.

## Serve Applications And Keep Context

| I want to… | CLI / HTTP | Guide |
| --- | --- | --- |
| Expose managed workflows over HTTP | `daemon` | [Daemon, host, port, bearer auth](./docs/user/daemon.md) |
| Keep one local/cloud model behind an HTTP endpoint | `server` | [Servers and lifecycle options](./docs/user/servers.md) |
| Route several model capabilities through one port | `cluster` | [Clusters](./docs/user/clusters.md) |
| Keep bounded local conversation context | `session`, `chat --session` | [Sessions](./docs/user/sessions.md) |
| Poll, cancel, collect results, and delete jobs | `/v1/jobs` | [Jobs](./docs/user/jobs.md) |

## Integrate A Client

- [HTTP index](./docs/user/api.md): native routes, authentication, errors, and payload links.
- [Provider compatibility](./docs/user/provider-compatibility.md): supported endpoint and field matrix.
- [OpenAI examples](./docs/user/providers/openai.md): chat, embeddings, images, audio, Python/JavaScript SDKs.
- [Anthropic / Claude examples](./docs/user/providers/anthropic.md): messages, streaming, Python SDK.
- [Gemini examples](./docs/user/providers/gemini.md): generate content, streaming, embeddings.
- [Base URL selection](./docs/user/providers/README.md#base-urls): daemon, local server, or cloud server.

## Support And Releases

- [Version notes](./docs/user/version.md): released behavior, limits, and upgrade expectations.
- [Stability checklist](./docs/user/1.0-readiness.md): the 1.0 promise and post-1.0 boundaries.
- [API stability contract](./docs/contracts/api-surface-stability.md): stable and experimental classifications.

## Common Workflows

### Customize A Chat Model

1. [Prepare and validate a dataset](./docs/user/datasets.md).
2. [Create a LoRA plan](./docs/user/training-lora.md#plan-commands) with a compatible local model.
3. [Run training](./docs/user/training-lora.md#run-and-select-the-adapter) and inspect the imported adapter.
4. [Compare chat with and without the adapter](./docs/user/inference/chat.md), then use the same adapter in server requests.

Plan creation only saves a recipe. The training guide explains backend limits,
progress reporting, and interruption behavior before you start a run.

### Connect An Application

1. Start the [daemon](./docs/user/daemon.md) for managed workflow APIs, or a
   [model server](./docs/user/servers.md) for a dedicated inference endpoint.
2. Select the [native HTTP format](./docs/user/api.md) or a
   [provider-compatible client](./docs/user/providers/README.md).
3. Use [job status and result routes](./docs/user/jobs.md) for asynchronous workflows.

For a quick daemon health check:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
curl -sS http://127.0.0.1:8790/healthz
tentgent daemon status
```

Daemon `/v1/*` routes require a bearer header when a token is configured.
`--host` selects the listener interface; it is separate from runtime-home
selection. See [binding and authentication](./docs/user/daemon.md).

### Route Multiple Models

A [Cluster](./docs/user/clusters.md) maps chat, embedding, rerank,
transcription, and vision routes to explicit local models. Validate and apply
its TOML definition, then launch it through the existing server lifecycle.
The guide covers readiness, hot reload, blockers, and recovery.

## Storage And Support

Runtime data belongs to the resolved Tentgent runtime home, which can be
overridden with `TENTGENT_HOME`. Models, adapters, datasets, and server specs
have managed identities. Learn the [runtime layout](./docs/user/runtime.md#runtime-home)
before relocating data or selecting a separate home.

Use `tentgent doctor` for a compact health report and
`tentgent model inspect <model-ref>` for model-specific support evidence.
Follow [maintenance guidance](./docs/user/maintenance.md) for failed imports,
stale ownership, and interrupted jobs.

The [version notes](./docs/user/version.md) describe release-specific behavior.
Runtime/catalog recognition alone does not establish successful execution:
read [support evidence](./docs/user/models.md) and verify your selected model.
Provider-compatible APIs implement the documented subset of each protocol.

## Contributing And Project Direction

For source builds and repository-local tests, start with the
[developer guide](./docs/development/README.md).
Cross-module and HTTP boundaries are indexed under
[contracts](./docs/contracts/README.md); agent-oriented repository context is in
[AGENTS.md](./AGENTS.md).

Track upcoming work in the [active plans index](./docs/plans/README.md),
[1.x roadmap](./docs/plans/v1.x-roadmap.md), and
[maintenance queue](./docs/plans/bugfix-maintenance-plan.md).
Completed plans remain in the [archive](./docs/plans/archive/README.md).

Report a reproducible problem or propose a focused improvement through
[GitHub Issues](https://github.com/HiroLiang/tentserv-agent/issues).
Include the Tentgent version, platform, selected backend, and relevant command
or API error while keeping credentials out of the report.

## License

Tentgent is licensed under [Apache License 2.0](./LICENSE).
Model weights and optional dependencies retain their own license terms.
