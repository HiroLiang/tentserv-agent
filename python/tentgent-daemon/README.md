# Tentgent Daemon

Use this directory as the standalone Python subproject for Tentgent runtime and daemon work.

## Directory Map

- `pyproject.toml`
  Python packaging entry point for this subproject.
- `src/tentgent_daemon/`
  Importable package root.
- `src/tentgent_daemon/runtime/`
  Stored-model resolution, runtime request types, and backend routing.
- `src/tentgent_daemon/cli/`
  Package-local CLI entry points such as `tentgent-chat-once` and `tentgent-server`.
- `src/tentgent_daemon/backends/`
  Backend-specific runtime integrations such as `mlx`, `transformers + peft`, and `llama_cpp`.
- `src/tentgent_daemon/server/`
  Long-lived server configuration and HTTP skeleton logic.
- `src/tentgent_daemon/providers/`
  Cloud provider request/response normalization for OpenAI and Anthropic.
- `src/tentgent_daemon/tools/`
  Internal helper tools such as the Hugging Face snapshot helper used by the Rust model-store MVP.

## Runtime Conventions

- The daemon should use the shared Tentgent runtime-home rules documented in [docs/contracts/runtime-home.md](../../docs/contracts/runtime-home.md).
- The daemon should not treat the repository root as its persistent storage root.
- Repository-local testing should rely on `TENTGENT_HOME="$PWD/.tentgent"` rather than ad hoc relative paths.
- Prefer direct calls to `python/tentgent-daemon/.venv/bin/...` when manually testing Python entry points from the repository root; this avoids `uv` workspace warnings from the parent repo.
- `tentgent-chat-once` currently runs the `safetensors -> transformers`, `mlx -> mlx-lm`, and `gguf -> llama.cpp` paths, all with `--stream`.
- `tentgent-server` now provides the Slice 5 long-lived server skeleton with `GET /healthz` and `POST /v1/chat`.
- HTTP `stream=true` is intentionally not implemented yet; the server returns `501` until the streaming protocol is chosen.
- `tentgent-server` now applies explicit lifecycle policy:
  - eager load when `--lazy-load` is absent
  - load on first request when `--lazy-load` is present
  - release on later `/healthz` or `/v1/chat` access once `--idle-seconds` has expired
- Message inputs accept ordered `role:content` entries so the first manual harness can already carry multi-turn context.
- A verified small MLX test model is `mlx-community/Llama-3.2-1B-Instruct-4bit`.
- A verified small GGUF test model is `DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF`.

## Expansion Rules

- Keep backend-specific details in the closest backend folder under `src/tentgent_daemon/backends/`.
- Keep Python-direct runtime logic inside `src/tentgent_daemon/runtime/`.
- Keep cloud provider HTTP payload and response parsing inside `src/tentgent_daemon/providers/`.
- Keep reusable entry logic inside the package and expose commands through `pyproject.toml` entry points.
- Keep internal helper tools inside `src/tentgent_daemon/tools/` instead of ad hoc top-level scripts.
- Add a local `README.md` or `AGENTS.md` in a backend subtree when that backend grows large enough to need its own routing document.
- Update the relevant Markdown in the same change whenever runtime boundaries or routing behavior change.
