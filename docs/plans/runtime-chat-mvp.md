# Runtime Chat MVP

This plan defines how Tentgent should introduce runnable model backends and a first usable single-shot chat flow.

## Decision Summary

- Start with a Python-first runtime harness before wiring the full Rust `tentgent chat` surface.
- Keep one shared runtime request shape and route to backend-specific adapters internally.
- Do not expose three unrelated user flows. Expose one Tentgent chat flow with backend routing behind it.
- Stage LoRA support after basic chat works. Do not block single-shot chat on full dynamic adapter support.

## Goals

- Let an installed model chat once with provided context.
- Let developers manually test chat with Python before the Rust CLI wrapper exists.
- Reuse stored `model_ref` and model metadata from the managed model store.
- Preserve a clean path toward `tentgent server` and later dynamic LoRA mounting.

## Non-Goals

- Full TUI chat UX
- Full daemon orchestration in the first runtime step
- Dynamic LoRA support for every backend in phase one
- Multi-tenant server lifecycle management in phase one

## Runtime Strategy

Use one shared routing contract:

- Input:
  - `model_ref`
  - `messages`
  - optional generation settings
  - optional adapter reference
- Routing:
  - `primary_format = "mlx"` -> `mlx-lm`
  - `primary_format = "safetensors"` -> `transformers` with PEFT-ready structure
  - `primary_format = "gguf"` -> `llama-cpp-python`

This keeps backend differences inside Python adapters while preserving one Tentgent-facing entry shape.

## Execution Order

### Phase 1: Runtime metadata and router contract

- Add a small runtime plan contract document or module that resolves:
  - `model_ref -> model.toml`
  - `primary_format -> backend`
  - `variant source path -> load path`
- Keep this read-only and independent from daemon lifecycle.

### Phase 2: Python-first manual chat harness

- Add a Python entry point that can be run manually, for example:
  - `uv run --project python/tentgent-daemon tentgent-chat-once --model-ref <REF> --message "..."` or equivalent package entry
- Accept:
  - model ref
  - one or more messages
  - optional max tokens and temperature
- Resolve the stored model from Tentgent-managed storage and call the backend-specific adapter.

### Phase 3: Backend adapters

- Add three internal Python adapters:
  - `mlx`
  - `transformers`
  - `llama_cpp`
- Keep them behind one shared interface such as:
  - `load(model_record, options)`
  - `generate(messages, options)`
- Do not build daemon-only lifecycle abstractions yet.

### Phase 4: Rust CLI wrapper

- Add `tentgent chat <MODEL_REF>` in Rust.
- Have Rust call the Python runtime harness rather than reimplementing runtime logic.
- Keep the CLI UX simple:
  - one-shot prompt
  - optional repeated stdin loop later

### Phase 5: Server mode

- Add `tentgent server <MODEL_REF>` only after one-shot chat is stable.
- Treat server mode as a long-lived runtime process, not as a requirement for one-shot chat.

### Phase 6: LoRA follow-up

- Introduce backend-specific adapter policy after basic chat works.
- Prioritize:
  1. `safetensors + PEFT`
  2. `mlx`
  3. `llama-cpp-python`
- Keep the first chat MVP functional even when no adapter is attached.

## Suggested Python Structure

```text
python/tentgent-daemon/
├── pyproject.toml
└── src/
    └── tentgent_daemon/
        ├── cli/
        │   └── chat_once.py
        ├── runtime/
        │   ├── router.py
        │   ├── records.py
        │   └── chat.py
        ├── backends/
        │   ├── mlx.py
        │   ├── transformers_peft.py
        │   └── llama_cpp.py
        └── tools/
            └── hf_snapshot.py
```

## Backend Notes

- `mlx-lm`
  - Best for MLX-format models on Apple Silicon
  - Do not assume it is the universal runtime for `safetensors` or `gguf`
- `transformers`
  - Best baseline for `safetensors`
  - Natural place for PEFT-backed LoRA work later
- `llama-cpp-python`
  - Best baseline for `gguf`
  - Server support can come later

## CLI Milestones

1. `tentgent chat <MODEL_REF> --message "..."`
2. `tentgent chat <MODEL_REF>` with prompt input
3. `tentgent server <MODEL_REF>`

## Verification Plan

- Run the Python harness directly against one installed model of each supported primary format.
- Verify the router picks the expected backend from stored metadata.
- Verify one-shot chat works with a short multi-message context.
- Verify Rust `tentgent chat` can wrap the same harness without changing backend behavior.

## Current Recommendation

- Yes, implement the runtime layers in stages.
- Yes, start with Python so models can be tested directly before the Rust chat UX is finalized.
- Yes, keep the three backends separated internally.
- No, do not expose them as three unrelated user-facing products.

## Current Progress

- The Python package layout is in place under `python/tentgent-daemon/src/tentgent_daemon/`.
- `tentgent-chat-once` now executes the `safetensors -> transformers` path against stored models.
- `tentgent-chat-once --stream` now streams generated text to stdout for the transformers path.
- `tentgent-chat-once` now executes the `mlx -> mlx-lm` path against stored models.
- `tentgent-chat-once --stream` now streams generated text to stdout for the MLX path as well.
- `tentgent-chat-once` now executes the `gguf -> llama.cpp` path against stored models.
- `tentgent-chat-once --stream` now streams generated text to stdout for the GGUF path as well.
- `tentgent chat <MODEL_REF>` now wraps the Python chat harness from Rust.
- The Rust wrapper supports repeated `--message` inputs, `--stream`, and a single interactive prompt when `--message` is omitted.
