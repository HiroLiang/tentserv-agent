# Version Notes

This document summarizes the current user-facing version. It is not a changelog yet.

## v0.1.4

`v0.1.4` adds HTTP chat streaming through Server-Sent Events.

Added:

- `/v1/chat` supports `stream=true` with SSE `delta`, `done`, and `error` events.
- Local base-model servers can stream backend output incrementally.
- Compatible local adapters can stream through the same `adapter_ref` request shape as non-streaming chat.
- OpenAI and Anthropic cloud provider servers normalize provider stream deltas into the same Tentgent SSE response shape.
- Streaming request validation preserves JSON errors before SSE headers when adapter, auth, model, or request preflight fails.

Known limits:

- Cloud provider servers do not support request-time local adapters.
- Streaming chunks follow backend or provider tokenization and may not align to full words.
- Generated dataset splits are not deduplicated against each other yet.

## v0.1.3

`v0.1.3` hardens provider-assisted dataset synthesis for tuning-data workflows.

Added:

- `dataset synth` can generate `train.jsonl`, `valid.jsonl`, `test.jsonl`, and `eval_cases.jsonl` in one package with split-specific count options.
- `dataset synth --retries` / `-r` retries each split independently after invalid provider JSON, schema mismatches, or transient provider errors.
- Multi-split synthesis writes successful split files immediately, so later split failures preserve earlier work.
- Provider generation prompts now include split-specific JSONL shape examples and stronger `tentgent.chat.v1` rules.
- Provider output parsing can repair a narrow class of extra-brace JSON mistakes around tool-result records and reports the repair as a warning.
- Synthesis failures write split-scoped debug artifacts under `_debug/<split>/`.

Known limits:

- Generated train, valid, and test splits are not deduplicated against each other yet.
- Dataset synthesis quality still depends on provider output and should be validated before import or training.
- Cloud provider servers do not support request-time local adapters.

## v0.1.2

`v0.1.2` adds cloud provider server routing and provider-assisted dataset workflows.

Added:

- OpenAI and Anthropic keys can be verified through `auth status`.
- `server run openai:<model>` and `server run claude:<model>` can expose provider chat through the local `/v1/chat` surface.
- `dataset validate`, `dataset template`, `dataset synth`, and `dataset eval` help produce and review `tentgent.chat.v1` tuning data.
- server JSON responses preserve UTF-8 text for direct curl readability.

Known limits:

- Cloud provider servers do not support request-time LoRA adapters.
- Dataset synthesis quality still depends on provider output and should be validated before import or training.

## v0.1.1

`v0.1.1` is the first installable MVP target with macOS and Windows release artifacts.

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
- Windows x86_64 `.zip` artifact and PowerShell installer

Known limits:

- macOS and Windows x86_64 are the first packaged install targets
- MLX requires Apple Silicon macOS
- Windows runtime support is PEFT/safetensors-first; MLX is disabled
- HTTP chat is currently non-streaming
- `llama-cpp` external adapter execution is not implemented in this MVP
- macOS signing and notarization are deferred to a later slice

## Upgrade Expectations

Patch releases should preserve all user runtime data under `TENTGENT_HOME`.

If a future release needs a runtime metadata migration, `tentgent doctor` should detect that state and print the required next step before destructive changes are made.
