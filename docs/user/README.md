# User Guide

Start with a goal below. Each feature guide connects CLI examples and parameters
to its HTTP formats, results, limitations, and related features.
The English documents describe the code in this checkout; compare
`tentgent --version` and `tentgent <command> --help` when using another release.

For an alphabetical-style CLI lookup, use the [command index](./commands.md).
For complete request details, start with the [HTTP index](./api.md).

<a id="shortest-path"></a>
<a id="start-here"></a>
<a id="find-commands-parameters-and-payloads"></a>

## Install, Configure, And Diagnose

| I want to… | CLI | Guide |
| --- | --- | --- |
| Install, upgrade, or select a version | `--version` | [Installation](./install.md) |
| Prepare Python dependencies and select a backend | `runtime bootstrap/status` | [Runtime](./runtime.md) |
| Set keys or switch env/file/Keychain modes | `auth` | [Authentication](./auth.md) |
| Diagnose, repair stale ownership, or clean staging | `doctor`, `runtime reconcile`, `store gc` | [Maintenance](./maintenance.md) |

## Manage Models, Adapters, And Data

| I want to… | CLI | Guide |
| --- | --- | --- |
| Pull/import a model, inspect capabilities and proof | `model` | [Models](./models.md) |
| Import, bind, inspect, or remove an adapter | `adapter` | [Adapters](./adapters.md) |
| Generate, validate, import, evaluate, or export data | `dataset` | [Datasets](./datasets.md) |
| Find a small test model or supported model family | `model catalog` | [Fixtures](./model-fixtures.md), [support catalog](./model-support-catalog.md) |

## Run Inference

| I want to… | CLI | Guide |
| --- | --- | --- |
| Chat with a base model or adapter, optionally stream | `chat` | [Text chat](./inference/chat.md) |
| Embed text or rank documents | `embed`, `rerank` | [Embeddings and reranking](./inference/embedding-rerank.md) |
| Transcribe audio or generate speech | `transcribe`, `speak` | [Audio](./inference/audio.md) |
| Ask a question about an image | `vision chat` | [Vision](./inference/vision.md) |
| Ask a question about a video | `video understand` | [Video](./inference/video.md) |
| Create an image from text | `image generate` | [Image generation](./inference/images/generate.md) |
| Restyle, inpaint, or use ControlNet | `image transform/inpaint/control` | [Image editing](./inference/images/edit.md) |

<a id="media-workflow-rules"></a>

The [inference index](./inference/README.md) explains file inputs, output paths,
multipart uploads, and which HTTP operations produce jobs.

## Train And Use LoRA

Prepare a [dataset](./datasets.md), create and inspect a
[train plan](./training-lora.md#plan-commands), then
[run training and select the adapter](./training-lora.md#run-and-select-the-adapter).
The guide covers every `train lora` command, backend-specific parameters,
HTTP fields, logs, and current interruption/resume limits.

## Serve Applications And Keep Context

| I want to… | CLI / HTTP | Guide |
| --- | --- | --- |
| Expose managed workflows over HTTP | `daemon` | [Daemon, host, port, bearer auth](./daemon.md) |
| Keep one local/cloud model behind an HTTP endpoint | `server` | [Servers and lifecycle options](./servers.md) |
| Route several model capabilities through one port | `cluster` | [Clusters](./clusters.md) |
| Keep bounded local conversation context | `session`, `chat --session` | [Sessions](./sessions.md) |
| Poll, cancel, collect results, and delete jobs | `/v1/jobs` | [Jobs](./jobs.md) |

## Integrate A Client

- [HTTP index](./api.md): native routes, authentication, errors, and payload links.
- [Provider compatibility](./provider-compatibility.md): supported endpoint and field matrix.
- [OpenAI examples](./providers/openai.md): chat, embeddings, images, audio, Python/JavaScript SDKs.
- [Anthropic / Claude examples](./providers/anthropic.md): messages, streaming, Python SDK.
- [Gemini examples](./providers/gemini.md): generate content, streaming, embeddings.
- [Base URL selection](./providers/README.md#base-urls): daemon, local server, or cloud server.

## Support And Releases

- [Version notes](./version.md): released behavior, limits, and upgrade expectations.
- [Stability checklist](./1.0-readiness.md): the 1.0 promise and post-1.0 boundaries.
- [API stability contract](../contracts/api-surface-stability.md): stable and experimental classifications.

<a id="notes"></a>

## Documentation Boundaries

Reusable feature examples belong here. Personal interview scripts, local refs,
and generated data stay in the ignored `test-data/` directory.
[Engineering contracts](../contracts/README.md) define implementation boundaries;
[developer workflows](../development/README.md) cover source builds and tests.
Return to the [project README](../../README.md) for the short entry point.
