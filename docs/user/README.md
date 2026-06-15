# User Docs

This directory keeps user-facing Tentgent docs out of the root README so the project entry page stays short.

## Shortest Path

Use the CLI for local workflows and start the daemon only when you need HTTP:

```bash
tentgent runtime bootstrap
tentgent doctor
tentgent auth hf set
tentgent model pull google/gemma-3-1b-it
tentgent chat <model-ref> --message "user:Hello"
tentgent daemon start --host 127.0.0.1 --port 8790
```

The current tool is CLI plus daemon REST. There is no terminal UI command.

## Start Here

- [install.md](./install.md)
  Homebrew install, Linux x86_64 preview install, upgrade, uninstall, pinned
  versions, PATH notes, and local package smoke tests.
- [version.md](./version.md)
  Current version feature list, known limits, and release expectations.
- [commands.md](./commands.md)
  Common commands for auth, models, adapters, datasets, chat, media workflows,
  servers, daemon, sessions, and LoRA training.
- [api.md](./api.md)
  User-facing daemon HTTP API reference, including request shapes, result
  routes, job behavior, multipart media upload semantics, and HTTP error
  behavior.
- [provider-compatibility.md](./provider-compatibility.md)
  OpenAI, Claude/Anthropic, and Gemini-compatible endpoint support matrix.
- [provider-compatible-examples.md](./provider-compatible-examples.md)
  Copy-paste curl and SDK examples for currently supported provider-shaped
  Tentgent routes.
- [model-fixtures.md](./model-fixtures.md)
  Recommended small Hugging Face models and smoke-test commands for chat,
  embedding, rerank, audio transcription, and media workflows.
- [model-support-catalog.md](./model-support-catalog.md)
  Built-in model-family catalog, support hint levels, and how catalog evidence
  differs from local verification proof.
- [runtime.md](./runtime.md)
  Runtime home layout, environment overrides, daemon media upload limits,
  backend support, and macOS Keychain prompts.

## Media Workflow Rules

- CLI media commands such as `tentgent transcribe`, `tentgent speak`,
  `tentgent vision chat`, `tentgent image generate`, `tentgent image
  transform`, `tentgent image inpaint`, `tentgent image control`, and
  `tentgent video understand` read local files, text, or prompts from the
  caller's machine and run in the foreground.
- Daemon media endpoints receive multipart file bytes. `curl -F
  file=@/path/audio.mp3`, `curl -F image=@/path/image.png`, and `curl -F
  file=@/path/video.mp4` are client-side shorthand for reading local files; the
  daemon does not receive or trust the original client path.
- Audio transcription, audio speech, and image generation/editing daemon
  requests create workflow jobs. Video understanding also creates a workflow
  job. Vision chat daemon requests are bounded synchronous requests.
- Multipart audio and image uploads share
  `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`, which defaults to 20 MiB and returns HTTP
  `413` with `upload_too_large` when exceeded. Video uploads use the separate
  `TENTGENT_VIDEO_UPLOAD_MAX_BYTES` cap, which defaults to 512 MiB and returns
  `video_upload_too_large` when exceeded.
- Recommended small local model fixtures and copy-paste smoke commands live in
  [model-fixtures.md](./model-fixtures.md).

## Notes

- The root [README.md](../../README.md) is the short user entry point.
- Detailed runtime contracts stay under [docs/contracts/](../contracts/README.md).
- Developer-only source workflows stay under [docs/development/](../development/README.md).
