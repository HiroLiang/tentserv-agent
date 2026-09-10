# Inference

Prepare the [runtime](../runtime.md), select a [supported model](../models.md), then choose the input and output you need. [Small fixtures](../model-fixtures.md) provide repeatable smoke examples.

| Task | CLI | Guide |
| --- | --- | --- |
| Text conversation | `chat` | [Chat](./chat.md) |
| Text vectors and document scoring | `embed`, `rerank` | [Embeddings and reranking](./embedding-rerank.md) |
| Audio to text, text to WAV | `transcribe`, `speak` | [Audio](./audio.md) |
| Image plus text question | `vision chat` | [Vision](./vision.md) |
| Video plus text question | `video understand` | [Video](./video.md) |
| Generate or edit images | `image generate/transform/inpaint/control` | [Images](./images/README.md) |

## File And HTTP Media Rules

- Foreground CLI media commands read local paths on the caller's machine. They
  do not create daemon jobs unless the command explicitly says it talks to the
  daemon.
- When a CLI media command accepts `--output`, it writes to that path and fails
  before running if the file already exists. Without `--output`, text-like
  formats print to stdout when the format supports terminal output.
- Daemon media endpoints receive multipart file bytes, not client-local paths.
  `curl -F file=@/path/audio.mp3` and `curl -F image=@/path/image.png` are curl
  syntax for reading local bytes into the request; the same applies to
  `curl -F file=@/path/video.mp4`.
- Audio transcription, video understanding, and image generation daemon routes
  create workflow jobs and expose result bytes or files through result routes.
  Native vision chat daemon uploads are bounded synchronous requests.
- Audio/image multipart uploads share `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`,
  defaulting to 20 MiB. Video uploads use `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`,
  defaulting to 512 MiB. Exceeding either cap returns HTTP `413`.

Daemon `/v1/*` requests require the [bearer header](../api.md) when a token is configured. Result formats and download routes belong to the specific feature guide; shared cancellation and cleanup are covered by [Jobs](../jobs.md).

## Repeated Requests

Use a [model server](../servers.md) or a [Cluster](../clusters.md) for persistent application access. [Provider-compatible APIs](../provider-compatible-examples.md) support selected SDK request shapes. Runtime/model retention depends on configured idle settings.
