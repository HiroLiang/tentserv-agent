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
  Show auth-group help with provider examples.
- `tentgent auth hf --help`
  Show Hugging Face auth help with action examples.
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
  Show model-group help with add, pull, list, and inspect examples.
- `tentgent model add <PATH>`
  Import a local file or directory into `TENTGENT_HOME/models`.
- `tentgent model pull <HF_REPO> [--revision <REV>]`
  Resolve and download a full Hugging Face snapshot into the managed model store.
- `tentgent model ls`
  List managed models by short ref, primary format, and source.
- `tentgent model rm <HASH>`
  Remove one managed model and its related source indexes by short or full hash ref.
- `tentgent model inspect <REF>`
  Show metadata, manifest path, and canonical store path for one managed model.
- `tentgent chat <MODEL_REF> [--message ...]`
  Run the Rust chat wrapper around the Python runtime harness.
- `tentgent chat <MODEL_REF>`
  Prompt once for a user message when no `--message` flags are provided.
- `tentgent chat <MODEL_REF> --stream`
  Preserve streamed stdout from the selected Python backend.
