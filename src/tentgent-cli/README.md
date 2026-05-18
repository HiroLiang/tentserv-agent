# Tentgent CLI

This crate owns the `tentgent` binary and should stay focused on command-line interaction.

## Responsibility

- Parse command-line arguments.
- Render help, status sections, progress output, and user-facing errors.
- Convert terminal input into kernel use-case requests or daemon lifecycle
  calls.
- Call shared kernel and daemon APIs instead of owning business rules.

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

## Current Surface

The CLI is the user-facing local operator. It currently exposes these command
families:

- `doctor`
  Run broad local diagnostics, including platform, backend, runtime, and
  footprint checks.
- `runtime`
  Inspect or bootstrap the managed Python runtime.
- `auth`
  Inspect, set, validate, or remove provider keys through environment and
  system keychain resolution.
- `model`, `adapter`, `dataset`
  Pull, import, list, inspect, bind, export, diff, or remove managed local
  assets.
- `chat`
  Run one-shot text chat through kernel chat use cases and the Python runtime
  harness.
- `server`
  Create, run, start, stop, inspect, and remove long-lived model-bound server
  specs.
- `daemon`
  Start, stop, inspect, and foreground-run the persistent Rust daemon host.
- `session`
  Create, inspect, append, compact, and remove local short-term chat sessions.
- `train`
  Plan and run LoRA workflows and inspect plan/run state.

The current product surface is CLI plus daemon REST. This crate must not
reintroduce a terminal UI command or place product orchestration outside kernel
or daemon use-case boundaries.
