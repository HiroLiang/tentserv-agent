# Command Index

Find a command, then open its feature guide for prerequisites, examples,
parameters, HTTP formats, and limits. Run `tentgent <command> -h` for compact
help or `--help` for extended descriptions of your installed version.

Replace angle-bracket placeholders with your own refs, paths, and values.
Managed resource selectors accept unique short refs where documented; stored
Cluster definitions require full canonical model refs. Option aliases are
command-specific: `-m`, `-p`, and `-n` do not have one global meaning.

For task-based navigation, use the [user guide](./README.md). For HTTP routes,
use the [API index](./api.md). Historical section links below remain valid.

## File And HTTP Media Rules

[File And HTTP Media Rules guide](./inference/README.md#file-and-http-media-rules).

## Auth

[Auth guide](./auth.md).

`tentgent auth status`, `tentgent auth mode`, `tentgent auth hf set`, `tentgent auth hf rm`, `tentgent auth openai set`, `tentgent auth openai rm`, `tentgent auth anthropic set`, `tentgent auth anthropic rm`, `tentgent auth gemini set`, `tentgent auth gemini rm`.

## Runtime

[Runtime guide](./runtime.md#common-commands).

`tentgent runtime bootstrap`, `tentgent runtime status`, `tentgent runtime reconcile`, `tentgent doctor`, `tentgent store gc`.

For `doctor`, `runtime reconcile`, and `store gc`, use [Diagnostics and maintenance](./maintenance.md).

<a id="models-and-chat"></a>

## Models

[Models guide](./models.md).

`tentgent model add`, `tentgent model pull`, `tentgent model catalog`, `tentgent model ls`, `tentgent model rm`, `tentgent model inspect`, `tentgent model capability show`, `tentgent model capability set`, `tentgent model capability add`, `tentgent model capability remove`, `tentgent model capability proofs`, `tentgent model capability verify`, `tentgent model capability proof clear`.

## Chat

[Chat guide](./inference/chat.md).

`tentgent chat`, `tentgent embed`, `tentgent rerank`.

For `embed` and `rerank`, use [Embeddings and reranking](./inference/embedding-rerank.md).

## Audio Transcription

[Audio Transcription guide](./inference/audio.md#audio-transcription).

`tentgent transcribe`.

## Audio Speech

[Audio Speech guide](./inference/audio.md#audio-speech).

`tentgent speak`.

## Vision Chat

[Vision Chat guide](./inference/vision.md).

`tentgent vision chat`.

## Video Understanding

[Video Understanding guide](./inference/video.md).

`tentgent video understand`.

## Image Generation

[Image Generation guide](./inference/images/README.md).

`tentgent image generate`, `tentgent image transform`, `tentgent image inpaint`, `tentgent image control`.

## Cluster

[Cluster guide](./clusters.md).

`tentgent cluster run`, `tentgent cluster apply`, `tentgent cluster validate`, `tentgent cluster ls`, `tentgent cluster inspect`, `tentgent cluster rm`.

## Server

[Server guide](./servers.md).

`tentgent server run`, `tentgent server ls`, `tentgent server ps`, `tentgent server inspect`, `tentgent server start`, `tentgent server stop`, `tentgent server rm`.

## Daemon

[Daemon guide](./daemon.md).

`tentgent daemon run`, `tentgent daemon start`, `tentgent daemon status`, `tentgent daemon stop`.

## Sessions

[Sessions guide](./sessions.md).

`tentgent session ls`, `tentgent session inspect`, `tentgent session messages`, `tentgent session create`, `tentgent session update`, `tentgent session append`, `tentgent session compact`, `tentgent session rm`.

## Adapters

[Adapters guide](./adapters.md).

`tentgent adapter add`, `tentgent adapter pull`, `tentgent adapter ls`, `tentgent adapter inspect`, `tentgent adapter bind`, `tentgent adapter rm`.

## Datasets

[Datasets guide](./datasets.md).

`tentgent dataset add`, `tentgent dataset validate`, `tentgent dataset template`, `tentgent dataset synth`, `tentgent dataset eval`, `tentgent dataset ls`, `tentgent dataset inspect`, `tentgent dataset export`, `tentgent dataset diff`, `tentgent dataset rm`.

## LoRA Training

[LoRA Training guide](./training-lora.md).

`tentgent train lora run`, `tentgent train lora plan create`, `tentgent train lora plan ls`, `tentgent train lora plan inspect`, `tentgent train lora plan rm`.
