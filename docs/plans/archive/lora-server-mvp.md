# LoRA Server MVP

Status: archived. This plan completed the adapter store, dataset store, LoRA train-plan flow, MLX/PEFT training execution, and request-time adapter selection for chat/server paths.

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
- Keep adapters, datasets, and training runs as separate managed resources instead of relying on one implicit current-model state.

## Goals

- Let one running server optionally answer a chat request with a selected LoRA adapter.
- Keep base-model chat working when no adapter is selected.
- Define a stable adapter-selection contract before adding hot-swap complexity.
- Preserve a clean path toward later adapter management and allowlist policy.
- Leave room for a future workflow of:
  - synthesize or import training data
  - train LoRA
  - test the result
  - adjust data or config
  - retrain
  - release accepted adapter versions

## Non-Goals

- Cross-server adapter sharing
- Multi-server orchestration
- Distributed coordination or shared in-memory state
- Automatic model-selected adapter choice in the first pass
- Dynamic remote adapter download during a live request
- A new hosted adapter marketplace that duplicates existing ecosystem registries

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

## Resource Model

Treat these as separate managed resource types:

- `model`
  - base model assets
- `adapter`
  - LoRA or other injectable adapter assets
- `dataset`
  - training or evaluation data
- `train run`
  - one recorded LoRA training execution and its output

Do not make operators rely on one hidden global "current model". Commands should prefer explicit references such as:

- `--base-model-ref`
- `--adapter-ref`
- `--dataset-ref`

## Managed Storage Shape

Tentgent should eventually manage:

- `TENTGENT_HOME/models/`
- `TENTGENT_HOME/adapters/`
- `TENTGENT_HOME/datasets/`
- `TENTGENT_HOME/train-runs/`

Adapter-store layout and metadata are drafted in [adapter-store.md](../contracts/adapter-store.md).

Adapters should not be physically nested under one base model directory by default. Instead, adapter metadata should record compatibility such as:

- `base_model_ref`
- `model_family`
- `adapter_format`
- `source_kind`
- `source_repo`
- `source_revision`
- `training_dataset_ref`
- `training_config_ref` or embedded config metadata

## External Source Strategy

Tentgent should not start by inventing a new remote adapter-hosting service.

Prefer this order:

1. local managed adapter store
2. pull adapters from external repositories such as Hugging Face
3. optional future publication flow back to Hugging Face or another registry

This means a future Tentgent adapter command surface should be able to:

- add local adapters
- pull remote adapters
- inspect compatibility metadata
- optionally publish or export later

## Dataset And Cloud-Assisted Workflow

Cloud API usage should be treated as a dataset-generation concern, not as the core LoRA runtime concern.

Tentgent should eventually support a workflow like:

1. import or synthesize a dataset
2. train a LoRA adapter against a chosen base model
3. test the adapter locally
4. adjust dataset content or training config
5. retrain
6. mark an adapter version as acceptable for release

This suggests a future split between:

- dataset management
- training execution
- adapter serving

Cloud APIs should therefore be considered for:

- synthetic example generation
- data cleanup or expansion
- evaluation assistance

Do not couple that directly to the first adapter-serving implementation.

## Proposed Command Surface

Start with explicit command groups rather than one overloaded `lora` command.

### Adapter commands

```text
tentgent adapter add <PATH>
tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>
tentgent adapter pull <REMOTE_REF>
tentgent adapter ls
tentgent adapter inspect <ADAPTER_REF>
tentgent adapter rm <ADAPTER_REF>
```

Possible later extensions:

```text
tentgent adapter verify <ADAPTER_REF>
tentgent adapter export <ADAPTER_REF>
tentgent adapter publish <ADAPTER_REF>
```

### Dataset commands

```text
tentgent dataset add <PATH>
tentgent dataset ls
tentgent dataset inspect <DATASET_REF>
tentgent dataset rm <DATASET_REF>
```

Possible later cloud-assisted extensions:

```text
tentgent dataset synth --provider <PROVIDER> --model <MODEL> ...
tentgent dataset eval <DATASET_REF> ...
```

### Training commands

```text
tentgent train lora --base-model-ref <MODEL_REF> --dataset-ref <DATASET_REF> ...
tentgent train ls
tentgent train inspect <RUN_REF>
tentgent train rm <RUN_REF>
```

Possible later release-oriented extensions:

```text
tentgent train retry <RUN_REF>
tentgent train promote <RUN_REF>
```

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

### Phase 3.5: Dataset and training surface planning

- Define the first dataset and training command surfaces.
- Keep this focused on resource identity and metadata, not on full training execution yet.

### Phase 4: First backend implementation

- Implement request-time adapter loading for `safetensors + PEFT`.
- Keep the first version conservative:
  - one active request at a time
  - explicit load/use/release behavior
  - no cross-request hot-swap optimization yet
- Status:
  - in place for request-time PEFT adapter selection on the transformers backend

### Phase 5: Server-side adapter policy

- Add server-visible adapter inventory rules such as:
  - allowed adapters
  - default adapter
  - load-on-demand or preload

### Phase 6: Follow-up backend parity

- Add backend-specific support or explicit unsupported behavior for:
  - `mlx`
  - `llama-cpp-python`
- status:
  - in place for conservative MLX adapter execution via model reload
  - `llama-cpp-python` remains explicitly unsupported for external adapter execution

## Review-Sized Implementation Slices

Build the first LoRA milestone in this order:

### Adapter Add Slice 1: command shape

- add `tentgent adapter add <PATH>` to the Rust CLI
- keep output as a parsed scaffold only
- do not write adapter store files yet
- goal:
  - lock command naming, help text, and argument shape

### Adapter Add Slice 2: store path and metadata types

- add the first Rust core adapter module
- define adapter-store paths under `TENTGENT_HOME/adapters/`
- define the first `adapter.toml` metadata type
- do not copy adapter files yet
- goal:
  - make the storage and metadata boundary reviewable
- status:
  - in place

### Adapter Add Slice 3: local import

- copy a local PEFT adapter directory into staging
- build a manifest and content-derived `adapter_ref`
- write `adapter.toml`, `manifest.json`, and source indexes
- goal:
  - complete the first managed adapter import path
- status:
  - in place for local PEFT-style and MLX-style adapter directories

### Adapter Add Slice 3.5: base model binding

- add `--base-model-ref <MODEL_REF>` to `tentgent adapter add`
- resolve the local managed model before writing adapter metadata
- read adapter config base-model hints when available
- reject obvious base-model mismatches
- write `base_model_ref`, `base_model_source_repo`, and `base_model_source_revision` into `adapter.toml`
- goal:
  - make adapter compatibility explicit before server runtime use
- status:
  - in place

### Adapter Add Slice 4: list and inspect

- add `tentgent adapter ls`
- add `tentgent adapter inspect <ADAPTER_REF>`
- goal:
  - make imported adapters visible before server runtime integration
- status:
  - in place

### Adapter Add Slice 4.5: explicit base binding

- add `tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>`
- allow imports to remain unbound when the compatible base model is not yet installed
- validate adapter config hints against the selected local base model before writing the binding
- update `adapter.toml` and `by-base/<base_model_ref>/<adapter_ref>.toml`
- goal:
  - avoid prompt-driven or network-driven side effects during adapter import
- status:
  - in place

### Adapter Add Slice 5: remove

- add `tentgent adapter rm <ADAPTER_REF>`
- block removal when a stored server spec explicitly references the adapter
- goal:
  - make adapter lifecycle safe before runtime use
- status:
  - in place

### Adapter Pull Slice 1: Hugging Face snapshot import

- add `tentgent adapter pull <HF_REPO> [--revision <REV>] [--base-model-ref <MODEL_REF>]`
- reuse the shared Python `tentgent-hf-snapshot` helper
- write Hugging Face source metadata and `by-source/hf` index entries
- reuse existing adapter manifest, hashing, format detection, and optional base-model binding
- goal:
  - make external PEFT adapter repos importable before server runtime integration
- status:
  - in place

### Slice 1: request contract only

- add optional `adapter_ref` to the HTTP chat contract
- thread it through the Rust and Python surfaces
- validate adapter existence, model compatibility, and backend support before generation
- return explicit errors for missing, incompatible, unsupported, or not-yet-implemented adapters
- do not execute adapter logic yet
- goal:
  - lock the user-facing request shape
- status:
  - in place

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
- status:
  - in place

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
- status:
  - in place for MLX adapter reload execution
  - `llama-cpp-python` remains unsupported in this MVP

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
- Remote adapter hosting already exists in the ecosystem, so Tentgent should favor interoperability before inventing a new hosted adapter platform.
