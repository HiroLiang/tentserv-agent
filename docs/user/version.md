# Version Notes

This document summarizes the current user-facing version. It is not a changelog yet.

## v0.3.0-alpha.1

`v0.3.0-alpha.1` is a TUI preview release. It makes the terminal UI useful as a
local operator console, but it is still an alpha while the interaction model is
being refined.

Added:

- `tentgent tui` operator workflows for chat, sessions, jobs, resources, stores,
  servers, and LoRA training surfaces.
- Background daemon job records and TUI progress surfaces for long-running
  store/dataset actions without changing existing synchronous API behavior.
- Picker-first server and LoRA plan creation flows with review/preview pages.
- Guarded TUI actions for model, adapter, dataset, server, training, and session
  management through existing daemon routes.
- Compact session/server/adapter ref display in dense TUI and CLI session lists.
- More reliable detached local server startup, including bind preflight and
  early health/process observation.

Known limits:

- This is an alpha release. The TUI is usable for workflow testing, but layout,
  key bindings, picker flows, and action review screens may still change before
  a stable release.
- `latest` installers may still be managed separately from prerelease adoption;
  use the explicit `v0.3.0-alpha.1` release URL when testing this preview.
- Server, training, store, and session mutations remain guarded, but the TUI UX
  is still being tightened after real smoke tests.
- macOS signing and notarization are still deferred.

## v0.2.0

`v0.2.0` expands the local HTTP daemon from a chat surface into a programmatic
peer for the main CLI workflows.

Added:

- HTTP store parity for model, adapter, dataset, and stopped server inspect,
  import/pull, bind, and remove workflows.
- HTTP dataset tooling for validation, templates, provider-backed synthesis,
  provider-backed evaluation, export, and diff.
- HTTP LoRA plan and run APIs, including durable run records, metrics, and raw
  log inspection.
- HTTP auth status, observational doctor diagnostics, daemon logs, server logs,
  and token-gated daemon shutdown.
- HTTP and CLI session mutation, session-aware chat, OpenAI-compatible
  `session_ref` extensions, and destructive bounded session compaction.
- Terminal UI operator console with status/settings screens, daemon discovery,
  explicit daemon start, non-secret config, guarded local Keychain setup, and
  read-only navigators for stores, servers, sessions, and LoRA training state.

Known limits:

- Provider key set/remove remains local-only through the CLI or guarded TUI
  Keychain setup. No daemon HTTP secret mutation route exists.
- `doctor --fix` remains CLI-only; HTTP doctor is observational.
- `daemon start` is the primary background entry point for the HTTP daemon;
  foreground `daemon run` remains available for debugging.
- Cloud provider servers do not support request-time local adapters.
- Generated dataset splits are not deduplicated against each other yet.
- macOS signing and notarization are deferred to a later slice.

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
