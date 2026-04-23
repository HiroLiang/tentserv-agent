# Packaging And Install MVP

This plan records the future packaging and installation track after the current source-first development workflow.

## Scope

- Define the first product-shaped installation target for Tentgent.
- Keep the Rust CLI as the primary user-facing entry point.
- Preserve a clean Rust-to-Python runtime boundary so the current development layout can evolve into a packaged release.

## Goals

- Decide what a Tentgent release artifact should contain.
- Decide how end users should install Tentgent without working directly from the repository.
- Keep user-facing install steps short and stable.

## Candidate Packaging Tracks

### Track A: bundled Python runtime

- Ship a controlled Python runtime inside the Tentgent product bundle.
- Keep the Python daemon and runtime assets inside the packaged installation.
- Let the Rust CLI launch that bundled runtime.

### Track B: packaged daemon executable

- Expose the Python daemon through a packaged executable boundary.
- Let the Rust CLI launch a stable daemon executable instead of a Python module path.
- Keep the current Rust-to-Python boundary compatible with this future shape.

## Current Recommendation

- Keep both Track A and Track B open for now.
- Favor a clean runtime boundary first, not a rushed packaging decision.
- Do not force the current development `.venv` shape into the final release shape.

## Non-Goals

- Do not implement packaging yet.
- Do not decide the final release channel yet.
- Do not block LoRA or server follow-up work on packaging.

## Future Questions

- Should the first supported install channel be a downloadable archive plus installer script?
- Should Homebrew become the first polished install channel for macOS users?
- Should the first packaged release embed Python directly, or wrap the daemon behind an executable boundary?
