# Tentgent CLI

This crate owns the `tentgent` binary and should stay focused on command-line interaction.

## Responsibility

- Parse command-line arguments.
- Render help, status sections, progress output, and user-facing errors.
- Convert terminal input into core requests.
- Call shared core logic instead of owning business rules.

## Structure

- `src/main.rs`
  Binary entry point.
- `src/cli/app.rs`
  Root `clap` parser definition.
- `src/cli/commands/`
  Top-level command modules and command grouping.
- `src/cli/mod.rs`
  CLI entry and dispatch glue.

## Package Boundary Rule

- Do not create one Rust package per command by default.
- Keep commands as modules inside this crate while they share the same binary, output layer, and dependency set.
- Split a command family into a separate crate only when it has a genuinely separate runtime boundary, a very different dependency set, or needs to be reused by multiple entry points.

## Current MVP

- The current MVP includes auth-key management plus model-store basics.
- `tentgent`
  Show root help when no arguments are provided.
- `tentgent --help`
  Show root help explicitly.
- `tentgent help`
  Show root help through the built-in help subcommand.
- `tentgent auth --help`
  Show auth-group help.
- `tentgent auth hf --help`
  Show Hugging Face auth help.
- `tentgent auth hf set --help`
  Show the secure-input format and behavior for key storage.
- `tentgent auth hf`
  Show Hugging Face key status with `.env/env` and keychain resolution.
- `tentgent auth hf set`
  Prompt for a key, store it in the system keychain, and attempt validation.
- `tentgent auth hf rm`
  Remove the stored Hugging Face key from the system keychain.
- `tentgent auth openai`
  Show OpenAI key status with the same resolution rules.
- `tentgent auth anthropic`
  Show Anthropic key status with the same resolution rules.
- `tentgent model --help`
  Show model-group help.
- `tentgent model add <PATH>`
  Import a local file or directory into `TENTGENT_HOME/models`.
- `tentgent model pull <HF_REPO> [--revision <REV>]`
  Resolve and download a full Hugging Face snapshot into the managed model store.
- `tentgent model ls`
  List managed models by short ref, primary format, and source.
- `tentgent model rm <HASH>`
  Remove one managed model and its related source indexes by short or full hash ref. Removal is blocked while any stored server spec still references that model.
- `tentgent model inspect <REF>`
  Show metadata, manifest path, and canonical store path for one managed model.
- `tentgent adapter add <PATH> [--base-model-ref <MODEL_REF>]`
  Import one local adapter directory into the managed adapter store, optionally binding it to a local managed base model.
- `tentgent adapter pull <HF_REPO> [--revision <REV>] [--base-model-ref <MODEL_REF>]`
  Pull a Hugging Face adapter snapshot into the managed adapter store, optionally binding it to a local managed base model.
- `tentgent adapter ls`
  List managed adapters by short ref, format, base binding, and source.
- `tentgent adapter inspect <ADAPTER_REF>`
  Show metadata, manifest path, and canonical store path for one managed adapter.
- `tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>`
  Bind an already imported adapter to one local managed base model after validating adapter config hints when available.
- `tentgent adapter rm <ADAPTER_REF>`
  Remove one managed adapter and its related source or base-model indexes by short or full hash ref.
- `tentgent chat <MODEL_REF> [--message ...]`
  Run the Rust chat wrapper around the Python runtime harness.
- `tentgent chat <MODEL_REF>`
  Prompt once for a user message when no `--message` flags are provided.
- `tentgent chat <MODEL_REF> --stream`
  Preserve streamed stdout from the selected Python backend.
- `tentgent server run <MODEL_REF> [--home ...] [--host ...] [--port ...] [--lazy-load] [--idle-seconds ...] [--detach]`
  Persist a stable `server.toml` and launch the Python server skeleton. Foreground is the default, and `--detach` performs the initial launch in background mode.
- `tentgent server ls [--home ...]`
  List persisted server specs together with their current runtime state.
- `tentgent server ps [--home ...]`
  List only currently running server processes.
- `tentgent server inspect <SERVER_REF> [--home ...]`
  Show one stored server spec, runtime status, and server-local log paths.
- `tentgent server start <SERVER_REF> [--home ...] [--details]`
  Launch one stored server spec in background mode. The default output is concise, and `--details` adds the full inspection table.
- `tentgent server stop <SERVER_REF> [--home ...] [--details]`
  Stop one live server process without deleting its spec. The default output is concise, and `--details` adds the full inspection table.
- `tentgent server rm <SERVER_REF> [--home ...] [--details]`
  Remove one stopped server spec directory. The default output is concise, and `--details` adds the full inspection table captured before removal.
