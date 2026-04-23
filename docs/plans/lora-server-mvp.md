# LoRA Server MVP

This plan defines the next active runtime track after the completed single-model server MVP: request-time LoRA support on top of one long-lived server process.

## Scope

- Add LoRA-aware request and session boundaries to `tentgent server`.
- Keep the first LoRA track focused on one server process serving one base model.
- Prefer review-sized implementation slices over one large adapter branch.

## Decision Summary

- Build LoRA on top of the completed server lifecycle instead of before it.
- Keep request-time adapter selection explicit with `adapter_ref`.
- Keep adapter inventory and policy in the control plane, not inside the core chat payload.
- Prioritize one backend-first LoRA path before broad multi-backend parity.

## Goals

- Let one running server optionally answer a chat request with a selected LoRA adapter.
- Keep base-model chat working when no adapter is selected.
- Define a stable adapter-selection contract before adding hot-swap complexity.
- Preserve a clean path toward later adapter management and allowlist policy.

## Non-Goals

- Cross-server adapter sharing
- Multi-server orchestration
- Distributed coordination or shared in-memory state
- Automatic model-selected adapter choice in the first pass
- Dynamic remote adapter download during a live request

## Why LoRA Before Multi-Server Coordination

- LoRA is a direct extension of the current single-server runtime boundary.
- Multi-server coordination would introduce a separate systems track:
  - shared registry
  - network-visible state
  - coordination semantics
  - failure and recovery rules
- Tentgent should first prove:
  - where adapters live
  - how requests specify them
  - how one server loads, reuses, and releases them safely

## First-Pass Contract

Keep the first request shape simple:

- `messages`
- optional generation settings
- optional `adapter_ref`

Keep adapter inventory outside the request body:

- server spec may later include:
  - `allowed_adapters`
  - preload policy
  - lazy adapter load policy
- the core chat request should only say:
  - use this adapter
  - or use no adapter

## Backend Priority

Prioritize the first real LoRA path in this order:

1. `safetensors + PEFT`
2. `mlx`
3. `llama-cpp-python`

Reason:

- `safetensors + PEFT` is the most natural place for explicit adapter load, set, unload, and future hot-swap behavior.
- `mlx` and `llama-cpp-python` should stay behind the same contract, but they should not block the first adapter implementation.

## Execution Order

### Phase 1: Adapter contract

- Define the server-side request contract for `adapter_ref`.
- Define the runtime-session interface for optional adapter use.
- Keep the first implementation backend-limited if necessary.

### Phase 2: Adapter store and lookup shape

- Define where managed adapters live under `TENTGENT_HOME/adapters/`.
- Define the minimum metadata needed to resolve:
  - adapter identity
  - compatible base model or family
  - adapter format

### Phase 3: Request validation

- Reject requests that ask for:
  - missing adapters
  - incompatible adapters
  - adapters not allowed by the current server policy

### Phase 4: First backend implementation

- Implement request-time adapter loading for `safetensors + PEFT`.
- Keep the first version conservative:
  - one active request at a time
  - explicit load/use/release behavior
  - no cross-request hot-swap optimization yet

### Phase 5: Server-side adapter policy

- Add server-visible adapter inventory rules such as:
  - allowed adapters
  - default adapter
  - load-on-demand or preload

### Phase 6: Follow-up backend parity

- Add backend-specific support or explicit unsupported behavior for:
  - `mlx`
  - `llama-cpp-python`

## Review-Sized Implementation Slices

Build the first LoRA milestone in this order:

### Slice 1: request contract only

- add optional `adapter_ref` to the HTTP chat contract
- thread it through the Rust and Python surfaces
- do not execute any adapter logic yet
- goal:
  - lock the user-facing request shape

### Slice 2: adapter metadata shape

- define the first adapter metadata format
- define where adapter records live under `TENTGENT_HOME/adapters/`
- do not load adapters yet
- goal:
  - make adapter identity and compatibility explicit

### Slice 3: request validation

- reject missing or incompatible `adapter_ref`
- surface clear user-facing errors
- goal:
  - prove the control-plane contract before touching runtime loading

### Slice 4: PEFT-backed server execution

- implement the first request-time LoRA path for the transformers backend
- keep one active request at a time
- goal:
  - complete the first real LoRA-backed server chat flow

### Slice 5: adapter policy in server spec

- extend the server spec with adapter allowlist policy
- keep it optional in the first pass
- goal:
  - separate request choice from server authority

### Slice 6: follow-up backend status

- document or implement the first supported story for:
  - `mlx`
  - `llama-cpp-python`
- goal:
  - avoid pretending all backends have identical LoRA behavior

## Verification Plan

- Start one server for a compatible `safetensors` base model.
- Send one chat request with no adapter and verify base behavior remains unchanged.
- Send one chat request with a valid `adapter_ref` and verify the adapter path is actually used.
- Send one chat request with an invalid or incompatible `adapter_ref` and verify the server rejects it cleanly.
- Verify that stopping and restarting the server preserves adapter policy from the stored server spec.

## Future Direction

- Multi-server coordination should be a later systems plan, not part of this one.
- Shared network-visible server state should be designed separately from adapter execution.
- Packaging and install should remain tracked in [packaging-install-mvp.md](./packaging-install-mvp.md).
