# Version Notes

This document summarizes the current user-facing version. It is not a changelog yet.

## v0.1.0

`v0.1.0` is the first installable MVP target.

Included:

- provider auth key management for Hugging Face, OpenAI, and Anthropic
- content-addressed model store with local import and Hugging Face pull
- content-addressed adapter store with local import, Hugging Face pull, and train-run import
- content-addressed dataset store with import, export, diff, remove, and canonical chat schema support
- one-shot local chat for MLX, PEFT safetensors, and llama-cpp GGUF paths
- local HTTP chat server with server registry and process lifecycle commands
- managed LoRA train plans
- runnable MLX LoRA training loop
- runnable PEFT safetensors LoRA training loop
- installer-managed Python runtime bootstrap for normal installs

Known limits:

- macOS is the first supported install target
- MLX requires Apple Silicon macOS
- HTTP chat is currently non-streaming
- Windows is planned but not a release-supported target yet
- `llama-cpp` external adapter execution is not implemented in this MVP
- macOS signing and notarization are deferred to a later slice

## Upgrade Expectations

Patch releases should preserve all user runtime data under `TENTGENT_HOME`.

If a future release needs a runtime metadata migration, `tentgent doctor` should detect that state and print the required next step before destructive changes are made.
