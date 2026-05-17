# Kernel Architecture

This contract defines the internal shape of `src/tentgent-kernel` while the
project migrates behavior out of `tentgent-core`.

The current kernel is intentionally incremental. It should make package
boundaries and shared data objects obvious first, then move behavior one
coherent bundle at a time.

## Top-Level Shape

`tentgent-kernel` has three top-level areas:

```text
foundation/      low-level shared facts and path data
capabilities/    machine capability domain data
features/        product feature packages
```

Rules:

- `foundation` contains shared primitives and machine facts. It does not know
  product workflows such as model import, server launch, session chat, or
  training.
- `capabilities` contains shared readiness vocabulary. It is not a user-facing
  feature package.
- `features/*` maps to product areas and command families.
- CLI, HTTP, and TUI stay as input/rendering layers. They should not gain new
  ad hoc path, probe, or backend readiness logic while the migration is active.

## Current Package Shape

The source tree is the authority for exact files. Current packages follow this
shape:

- `domain.rs` owns structures and enums.
- `ports.rs` owns narrow traits for package boundaries.
- `infra/` or `infra.rs` owns standard implementations that touch the local
  machine, filesystem, environment, or subprocesses.
- `usecases.rs` may exist as a placeholder in feature packages, but should not
  hide implementation logic before the relevant bundle is moved.
- `usecases/` owns capability-sized workflow implementations when a feature
  has real orchestration. If workflow ports would make package `ports.rs` too
  broad, keep those use-case ports in `usecases/port.rs`.

## Domain Files

Use `domain.rs` for:

- pure structs and enums
- stable names shared by later stores, probes, services, or use cases
- data that can be tested without filesystem, network, subprocess, or Python
  runtime access

Do not put these in `domain.rs`:

- filesystem reads or writes
- environment-variable lookup
- process spawning
- backend probing
- CLI/HTTP/TUI rendering
- hidden dependency injection

## Foundation

`foundation` owns low-level shared structures.

Current domain areas:

- `foundation/layout/domain.rs`: runtime home, data root, and standard path
  data objects.
- `foundation/layout/infra.rs`: `StdRuntimeLayoutResolver`, the standard
  implementation that resolves roots and derived paths.
- `foundation/layout/ports.rs`: `RuntimeLayoutResolver`, the trait for
  resolving runtime layout in read-only or create-capable modes.
- `foundation/layout/tests.rs`: explicit root, env root, read-only, and create
  mode tests.
- `foundation/platform/domain.rs`: OS, arch, libc, CPU, GPU, CUDA, and Metal
  fact objects.
- `foundation/platform/ports.rs`: `PlatformProbe`, the trait for reading
  current platform facts.
- `foundation/platform/infra.rs`: `StdPlatformProbe`, the standard
  implementation that reads current platform facts.
- `foundation/platform/tests.rs`: fake probe and standard probe smoke tests.
- `foundation/error.rs`: shared kernel errors and result alias.

Future implementation files may be added when the bundle moves:

```text
foundation/fs/
foundation/ids/
foundation/time/
```

These should remain internal helpers for shared mechanics, not product
workflow owners.

## Capabilities

`capabilities/domain.rs` owns machine readiness vocabulary:

- runtime profile readiness
- backend kinds
- backend readiness state
- machine capability state snapshots

`capabilities/ports.rs` defines the narrow boundaries for:

- probing machine capability state from runtime layout and platform facts
- loading and saving cached capability state
- checking backend and runtime-profile readiness for feature gates

Current standard implementations are `FileCapabilityStateStore`,
`StdMachineCapabilitiesProbe`, and `StdCapabilityChecker`. The standard
capability probe verifies runtime profile and backend dependencies by importing
the expected Python modules with the selected runtime interpreter. It still does
not launch models, start servers, run training jobs, or perform GPU allocation
smoke tests.
Capability probes receive the selected Python runtime layout when a caller has
already resolved it, so development-source and override environments do not get
misreported as missing just because they are outside `TENTGENT_HOME`.
Capability schema version `2` records this import-probed readiness semantics;
older cached capability files should be treated as stale by use cases.

`capabilities/usecases/port.rs` defines use-case-level ports for:

- resolving current or refreshed capability snapshots for callers without
  making CLI, HTTP, or TUI assemble layout and platform probes themselves
- enforcing backend and runtime-profile readiness without exposing checker
  details to feature packages

`capabilities/usecases/` owns orchestration implementations and their local
request/response structs. For example, `resolver.rs` keeps
`MachineCapabilitiesInput` and `MachineCapabilitiesSnapshot` next to
`StdMachineCapabilitiesResolver`. CLI, HTTP, and TUI should call use cases
instead of assembling layout, platform, cache, and probe steps themselves.

Use the term capability state for the data and cache. Do not add a separate
persisted-wrapper module unless a later migration proves that split removes
real complexity.

The likely persisted cache path remains:

```text
TENTGENT_HOME/runtime/capabilities.toml
TENTGENT_HOME/runtime/auth.toml
```

These files are local cached/state metadata, not user data identity.
`auth.toml` must contain only non-secret provider metadata.

## Features

Each feature package maps to a product area:

```text
features/auth/
features/model/
features/adapter/
features/chat/
features/dataset/
features/config/
features/server/
features/daemon/
features/doctor/
features/session/
features/runtime/
features/train/
```

Feature packages may eventually contain:

```text
domain.rs
infra/
store.rs
service.rs
runtime.rs
usecases.rs
usecases/
```

Only add these files when the feature needs them. Prefer a small, consistent
package over a theoretical Clean Architecture layout. If a file has no real
job yet, keep it empty or do not create it.

`mod.rs` and `lib.rs` are composition files only. They should declare modules,
re-export the public surface, and carry module-level documentation. Put runtime,
store, probe, planner, executor, and test logic in focused sibling files instead
of accumulating behavior inside a generic module entry file.

`features/runtime/domain.rs` owns pure runtime setup names and state:

- bootstrap profiles and resolved bootstrap plans
- Python runtime source and resolved Python project/environment layout
- Python entrypoint script names exposed by the daemon package
- runtime initialization/readiness snapshots

It must not spawn bootstrap scripts, run Python, inspect installed packages, or
read environment variables directly. Those jobs belong in runtime infra or
use cases once their migration bundle moves.

`features/model/domain.rs` owns pure model-store names and state:

- canonical SHA-256 `model_ref` and short/hash-prefix selectors
- model formats, serving capabilities, import methods, source kinds, and
  variant status names
- manifest, model metadata, source-index metadata, and import/removal result
  data objects
- model-store path derivation from an already resolved models directory
- pure format detection and primary-format selection rules from the model-store
  contract

It must not walk directories, hash files, copy/import model data, download from
Hugging Face, infer serving capabilities from remote metadata, read auth
secrets, inspect server references, or write metadata. Those jobs belong in
model infra and use cases when the model migration bundle moves.

`features/model/ports.rs` defines narrow boundaries for:

- ensuring model-store directories before mutating model operations
- staging local or remote model content before canonical identity is known
- fetching Hugging Face snapshots through an already selected Python runtime
- building manifests and deriving canonical `model_ref` values
- reading/writing model catalog metadata, manifests, variants, and source
  indexes
- moving/removing canonical model content
- checking stored server specs that block model removal

Model ports should receive resolved layout/runtime/auth inputs from use cases.
They should not resolve runtime-home, prompt for auth, decide CLI rendering, or
silently bootstrap Python runtime dependencies.

`features/model/infra/` owns the standard filesystem and subprocess adapters
for those ports: model-store directory creation, import staging, manifest
building, canonical manifest hashing, metadata/catalog reads and writes,
source-index cleanup, canonical content movement/removal, server-reference
checks, and Hugging Face snapshot helper execution through an already selected
Python runtime. The Hugging Face adapter may run the helper process, but it must
not resolve auth, bootstrap Python, or choose CLI progress rendering.

`features/model/usecases/port.rs` defines workflow boundaries for:

- listing and inspecting stored models without exposing catalog store details
- importing local model content through staging, manifesting, deduplication, and
  metadata/index writes
- pulling Hugging Face model content through resolved auth and Python runtime
  inputs while reporting progress through a caller-provided sink
- removing stored models after server-reference checks

Standard model use cases should compose foundation layout, runtime resolution,
auth secret resolution, and model infra ports. CLI, HTTP, and TUI should call
these use cases instead of rebuilding model-store orchestration directly.

`features/adapter/domain.rs` owns pure adapter-store names and compatibility
rules:

- canonical SHA-256 `adapter_ref` and short/hash-prefix selectors
- adapter formats, source kinds, backend-support names, manifest metadata, and
  source/base index metadata
- adapter-store path derivation from an already resolved adapters directory
- pure format detection for PEFT and MLX adapter layouts
- conservative compatibility checks against a selected chat-capable base model
  and backend

It must not walk directories, hash files, copy/import adapter data, download
from Hugging Face, read auth secrets, inspect server references, or write
metadata. Those jobs belong in adapter infra and use cases when the adapter
migration bundle moves.

`features/adapter/ports.rs` defines narrow boundaries for:

- ensuring adapter-store directories before mutating adapter operations
- staging local, Hugging Face, or training-run adapter content before canonical
  identity is known
- fetching Hugging Face snapshots through an already selected Python runtime
- building manifests and deriving canonical `adapter_ref` values
- reading source metadata hints from staged adapter content
- reading/writing adapter catalog metadata, manifests, and source/base indexes
- moving/removing canonical adapter content
- checking stored server specs that block adapter removal

Adapter ports should receive resolved layout/runtime/auth inputs from use
cases. They should not resolve runtime-home, prompt for auth, decide CLI
rendering, validate base-model compatibility by themselves, or silently
bootstrap Python runtime dependencies.

`features/adapter/infra/` owns the standard filesystem and subprocess adapters
for those ports: adapter-store directory creation, import staging, source
metadata reads from adapter config files, manifest building, canonical manifest
hashing, metadata/catalog reads and writes, source-index and base-index cleanup,
canonical content movement/removal, server-reference checks, and Hugging Face
snapshot helper execution through an already selected Python runtime. The
Hugging Face adapter may run the helper process, but it must not resolve auth,
bootstrap Python, validate base-model compatibility, or choose CLI progress
rendering.

`features/adapter/usecases/port.rs` defines workflow boundaries for:

- listing and inspecting stored adapters without exposing catalog store details
- importing local adapter content through staging, manifesting, deduplication,
  optional base-model binding, and metadata/index writes
- pulling Hugging Face adapter content through resolved auth and Python runtime
  inputs while reporting progress through a caller-provided sink
- importing successful training-run adapter output with training provenance
  indexes
- binding an existing adapter to one managed local base model
- validating an adapter for a selected server base model/backend target
- removing stored adapters after server-reference checks

Standard adapter use cases should compose foundation layout, runtime
resolution, auth secret resolution, model catalog reads, and adapter infra
ports. CLI, HTTP, TUI, training, and server preflight callers should call these
use cases instead of rebuilding adapter-store orchestration directly.
Current standard adapter use cases live in focused sibling files under
`features/adapter/usecases/`: catalog reads, local import, Hugging Face pull,
training-run import, base-model binding, compatibility checks, and removal.

`features/dataset/domain.rs` owns pure dataset-store names and schema state:

- canonical SHA-256 `dataset_ref` and short/hash-prefix selectors
- dataset formats, source kinds, split names, provider names, and template
  request data
- deterministic manifest, package metadata, validation outcome, diff outcome,
  import/export/removal result, and synth/eval request data
- dataset-store path derivation from an already resolved datasets directory

It must not walk directories, hash files, copy/import dataset data, call cloud
providers, read auth secrets, inspect training references, run Python, or write
metadata. Those jobs belong in dataset infra and use cases when the dataset
migration bundle moves.

`features/dataset/ports.rs` defines narrow boundaries for:

- ensuring dataset-store directories before mutating dataset operations
- staging local or generated dataset content before canonical identity is known
- building manifests and deriving canonical `dataset_ref` values
- detecting training package readiness and split metadata
- reading/writing dataset catalog metadata, manifests, and source indexes
- moving/exporting/removing canonical dataset content
- validating JSONL files or dataset directories against the canonical schema
- diffing stored or staged dataset manifests
- rendering editable Markdown-backed dataset templates
- executing provider-backed synth/eval workflows through an already selected
  Python runtime and auth secret
- checking train plans/runs that block dataset removal

Dataset ports should receive resolved layout/runtime/auth inputs from use
cases. They should not resolve runtime-home, prompt for auth, decide CLI/HTTP
rendering, silently bootstrap Python dependencies, or duplicate train-reference
policy in frontends.

`features/dataset/templates/` owns Markdown-backed templates that are meant to
be edited as text, such as dataset generation prompts. Rust code may include and
render these files, but long prompt bodies should live as `.md` templates rather
than string literals inside services or use cases.

`features/dataset/infra/` owns the standard filesystem, validation, template,
reference-guard, and subprocess adapters for dataset ports: dataset-store
directory creation, import/diff staging, manifest building, canonical manifest
hashing, package readiness detection, metadata/catalog reads and writes,
source-index cleanup, canonical content movement/export/removal, schema
validation, manifest diffing, Markdown template rendering, train plan/run
reference checks, and dataset synth/eval helper execution through an already
selected Python runtime. Runtime clients may build helper argv and parse helper
JSON/progress output, but they must not resolve auth, bootstrap Python, call
providers directly, or decide CLI/HTTP rendering.

`features/dataset/usecases/port.rs` defines workflow boundaries for:

- listing and inspecting managed datasets without exposing catalog store
  details
- importing local dataset content through staging, manifesting, deduplication,
  and metadata/index writes
- validating local or managed dataset content against the canonical schema
- rendering editable dataset templates and exact provider synthesis prompts
- running provider-backed dataset synthesis through resolved auth and Python
  runtime inputs
- running provider-backed dataset evaluation for a local path or managed
  dataset selector
- exporting, diffing, and removing managed dataset content while enforcing
  reference guards

Standard dataset use cases should compose foundation layout, runtime
resolution, auth secret resolution, and dataset infra ports. CLI, HTTP, TUI,
and training callers should call these use cases instead of rebuilding
dataset-store orchestration directly.
Current standard dataset use cases live in focused sibling files under
`features/dataset/usecases/`: catalog reads, local import, validation, template
rendering, provider-backed synthesis/evaluation, export, diff, and removal.

`features/chat/domain.rs` owns pure chat request and execution names:

- normalized chat roles and messages
- prompt and generation option data
- chat backend names used by local model execution
- resolved local-model and cloud-provider runtime targets
- resolved request-time adapter selection data
- response, finish-reason, and streaming-event data

It must not parse CLI flags, read HTTP bodies, inspect model or adapter stores,
spawn Python, proxy HTTP, read sessions, or write transcripts. Those jobs belong
in chat infra or use cases once the chat migration bundle moves.

`features/chat/ports.rs` defines narrow boundaries for:

- resolving a model selector into a chat-capable runtime target
- resolving and validating an adapter selector for a selected local chat target
- executing a prepared chat request through a selected Python runtime client

Chat ports should receive resolved layout/runtime data from use cases. They
should not resolve runtime-home, decide CLI/HTTP rendering, silently bootstrap
Python dependencies, or duplicate model/adapter catalog orchestration in
frontends.

`features/chat/infra/` owns the standard adapters for those ports: model target
resolution by adapting the model catalog use case, adapter target resolution by
adapting the adapter compatibility use case, and `tentgent-chat-once` process
execution through an already selected Python runtime. Chat infra may build
runtime argv and parse process output, but it must not prompt for auth, launch
servers, choose session context, or write chat transcripts.

`features/chat/usecases/port.rs` defines workflow boundaries for:

- preparing a chat request by resolving layout, Python runtime, model target,
  optional adapter target, prompt, and generation options
- running one-shot non-streaming chat generation through the prepared runtime
  request
- running streaming chat generation while forwarding normalized stream events
  through a caller-provided sink

Chat use cases should be the shared orchestration surface for CLI, HTTP, and
future TUI callers. Frontends should parse input and render output, while chat
use cases own model/adapter/runtime composition. Session transcript reads,
compaction, and writes should remain in session-specific workflows that call the
chat use cases with already selected context messages.
The current standard chat use case composes runtime resolution, chat model
resolution, chat adapter resolution, and a chat runtime client for prepare,
completion, and streaming flows.

`features/train/domain.rs` owns pure LoRA training names and planning rules:

- train refs and short/hash-prefix selectors
- LoRA backend request/selection names, plan/run statuses, and config sections
- plan, run, metrics-tail, raw-log, and store-layout data objects
- pure backend-selection defaults for MLX, PEFT, and blocked GGUF plans
- pure override application and automatic profile defaults

It must not read model or dataset catalogs, inspect files, write plan/run
records, spawn Python, import adapters, or render CLI/HTTP output. Those jobs
belong in train infra, train use cases, runtime infra, and frontend layers.

`features/train/ports.rs` defines narrow boundaries for:

- ensuring the LoRA train-store directory layout
- reading/writing train plans and run records
- initializing per-run metrics/raw-log artifacts
- probing persisted process liveness
- supplying timestamps and generated run refs
- launching hidden detached train workers

Train ports should receive resolved layout and selector inputs from use cases.
They should not resolve runtime-home, choose CLI rendering, bootstrap Python, or
import successful adapter output by themselves.

`features/train/infra/` owns the standard filesystem and local-process
adapters for those ports: train-store directory creation, TOML plan/run
catalogs, metrics/raw-log tails, process liveness probing, timestamp/run-ref
generation, and detached worker launch.

`features/train/usecases/port.rs` defines workflow boundaries for:

- previewing, creating, listing, inspecting, and removing LoRA train plans
- creating and updating durable LoRA run records
- listing/inspecting runs and reading bounded metrics/raw-log tails

Standard train use cases compose foundation layout/platform, model and dataset
catalog reads, train infra ports, and train planning rules. CLI should call
these use cases for plan/run state instead of using `tentgent-core` managers.
Foreground CLI execution may still own progress rendering while using train
use cases for durable state and adapter use cases for successful run imports.
HTTP train routes remain on the legacy path until the CLI migration is complete.

`features/runtime/ports.rs` defines narrow boundaries for:

- resolving Python project/environment layout from caller overrides, runtime
  home layout, packaged install candidates, development source, and environment
  policy
- resolving the managed Python binary and daemon entrypoint executable paths
- planning runtime bootstrap script invocation without spawning it
- executing runtime bootstrap from an explicit plan
- probing runtime initialization/readiness state

Runtime infra and use cases are still migration work; old `tentgent-core`
runtime helpers remain the behavior owner until their callers move. Current
runtime infra owns the standard Python runtime resolver, executable path
resolver, bootstrap planner/executor, and read-only runtime state probe.

`features/runtime/usecases/port.rs` defines orchestration boundaries for:

- resolving runtime-home layout and the effective Python runtime layout
- planning and executing managed runtime bootstrap through runtime infra
- probing managed runtime state without mutation
- resolving Python or daemon entrypoint executable paths for callers

Standard runtime use cases live in focused sibling files under
`features/runtime/usecases/`. They assemble foundation layout/platform ports
with runtime infra ports; CLI, HTTP, and doctor callers should depend on these
use cases instead of directly composing runtime infra.

`features/doctor/domain.rs` owns pure diagnostic report names and rules:

- doctor execution mode (`observational` vs local CLI)
- explicit repair intent such as developer Python environment sync
- check categories, pass/warn/fail/skipped status, summaries, and reports
- path, command, and repair-plan request data used by future doctor infra

`features/doctor/ports.rs` defines narrow boundaries for:

- filesystem path health checks without baking path logic into renderers
- external command checks such as developer `uv --version`
- mapping runtime state into doctor checks without reimplementing runtime
  bootstrap or Python runtime resolution
- mapping capability state into doctor checks without probing backends twice
- planning explicit repair actions separately from ordinary diagnostics

`features/doctor/infra/` owns the standard doctor adapters for those ports:
filesystem path probes, command probes, runtime fact-to-check mapping,
capability snapshot-to-check mapping, and repair planning. These adapters may
inspect local paths and run diagnostic commands, but they must not bootstrap the
runtime or execute repair steps directly.

`features/doctor/usecases/port.rs` defines workflow boundaries for:

- building a doctor report for local CLI, HTTP, or TUI callers without letting
  those frontends assemble path, command, runtime, and capability checks
- running an explicit repair flow that delegates mutation to runtime use cases
  and returns a fresh doctor report

Standard doctor use cases compose runtime state use cases, capability resolver
use cases, and doctor infra mappers. They may orchestrate diagnostic checks, but
must not hide runtime bootstrap inside ordinary report generation.

Runtime and doctor overlap only in the facts they inspect. Runtime owns
initialization and bootstrap planning/execution for the managed Python
environment. Doctor owns cross-system diagnosis and must not initialize runtime
during an observational report. `doctor --fix` style behavior should remain an
explicit repair intent that delegates to runtime bootstrap/planning/execution
code instead of hiding initialization inside diagnostics.

`features/config/domain.rs` owns pure user-config names and rules:

- config file name, schema version, and config section data
- daemon URL and token resolution source enums
- daemon endpoint formatting and default daemon endpoint values
- pure daemon URL/token precedence rules
- secret-like config key classification

It must not read environment variables, load or save TOML files, traverse TOML
values, or read daemon process metadata directly. Those jobs belong in config
infra or callers that map local state into config domain inputs.

`features/auth/domain.rs` owns pure provider-auth names and policy:

- provider names, CLI names, environment variables, and keychain accounts
- auth secret source, keychain presence, validation, and status data
- secret access intent and process-session cache policy
- secret material wrappers that redact debug output and clear owned secret
  memory on drop
- non-secret provider preferences

It must not read `.env`, environment variables, Keychain, or provider
validation endpoints directly. Keychain unlock details are not user-configurable
domain data; they belong inside the secret-store infra. Store implementations
should prefer one available non-password system unlock path first, then fall
back to the platform password prompt.

`features/auth/ports.rs` defines narrow boundaries for:

- probing environment-provided secrets from process env, cwd `.env`, or an
  explicit env file policy
- reading/writing/removing Keychain secrets
- validating provider secrets
- process-session secret caching
- loading/saving non-secret auth metadata

Current lightweight auth infra includes `ProcessSessionAuthSecretCache`,
`StdAuthEnvSecretProbe`, `InMemoryAuthMetadataStore`,
`FileAuthMetadataStore`, `SystemKeychainAuthSecretStore`, and
`ReqwestAuthSecretValidator`.
The keychain-backed secret store lives in `infra/store.rs`: store is the role,
while keychain names the secure operating-system backend. The macOS backend
uses Security Framework user-presence access control so protected entries can
prefer Touch ID and fall back to password. It prefers the Data Protection
Keychain when the current process has the required entitlement, and otherwise
uses the login Keychain with the same access control. If an unsigned
development binary lacks the required signing entitlements to create
user-presence items, the store may fall back to a standard login Keychain entry
so local auth remains usable. Windows and Linux use native `keyring` backends
and their operating-system prompt policy.
The store owns native unlock behavior; callers must not pass prompt
preferences through domain or use-case request data.
`ProcessSessionAuthSecretCache` is in-memory only and TTL-bounded; it must not
be replaced with a persisted secret cache.
File-backed auth metadata lives in `infra/metadata.rs` and writes
`runtime/auth.toml`; it must never serialize secret material.
Provider validation lives in `infra/validator.rs`: `reqwest` is the concrete
HTTP client, while the package boundary remains `AuthSecretValidator`.

`features/auth/usecases/` owns auth workflows by capability:

- `status.rs`: non-secret provider status assembly.
- `resolver.rs`: effective secret resolution using env, process TTL cache, and
  Keychain according to access policy, with explicit prompt/request-provided
  secrets resolved first when supplied by the caller.
- `mutation.rs`: local set/remove flows for Keychain secrets, non-secret
  metadata, and process cache.
- `validation.rs`: provider validation orchestration and metadata updates.
- `port.rs`: use-case-level traits for CLI, HTTP, TUI, and future server
  preflight callers.

Auth use cases may move secrets in memory, but they must not render, log,
serialize, or persist secret values outside the system Keychain.

The Rust CLI auth command now composes these kernel auth use cases directly.
The Rust CLI adapter command now composes kernel adapter infra and use cases
directly for local imports, Hugging Face pulls, catalog reads, binding, and
removal.
HTTP auth status, TUI auth setup, and core runtime provider-secret callers are
still migration callers until their entry points are switched to the same
kernel package.

## Dependency Direction

Allowed direction:

```text
features/* -> capabilities -> foundation
features/* -> foundation
```

Disallowed direction:

```text
foundation -> capabilities
foundation -> features/*
capabilities -> features/*
```

Cross-feature behavior should be explicit. If `server` later needs runtime
layout or capability state, pass those data objects or call a clear package
boundary. Do not hide feature-to-feature coupling inside probes or stores.

## Runtime Layout Rules

All runtime-home and standard path data should eventually flow through
`foundation/layout`.

Current layout package defines `RuntimeLayoutInput`, `RuntimeLayout`, and
`StdRuntimeLayoutResolver`.

The public layout shape should stay small:

- `home_dir`: control-plane root for config, sessions, servers, runtime, logs,
  locks, managed Python, bootstrap tools, and capability state.
- `data_root_dir`: data-plane root for models, adapters, datasets, training
  data, and cache.

If `data_root_dir` is unset, it resolves to `home_dir`. Avoid exposing many
per-directory path overrides as the main user-facing contract; advanced users
can use a different `home_dir`, a different `data_root_dir`, or OS-level
mounts/symlinks/junctions.

Implemented path resolution covers:

- `TENTGENT_HOME` / explicit `home_dir`
- `TENTGENT_DATA_ROOT` / optional explicit `data_root_dir`
- fixed standard subpaths under those roots
- read-only vs create-capable resolution mode

## Platform Rules

`PlatformFacts` describes machine facts only:

- OS and architecture
- Linux libc family/version
- CPU vendor, brand, and features
- GPU devices and hardware/runtime visibility such as CUDA or Metal

Platform facts do not mean a backend is ready. Backend readiness belongs to
capability state.

For example:

- CUDA visibility belongs in platform/GPU facts.
- “training backend can use CUDA” belongs in capability state.

## Persistence Rules

The source of truth remains file-based by default. SQLite may be added later as
a rebuildable index/cache, but it should not become the first source of truth
for models, adapters, datasets, sessions, servers, runtime state, or training
runs.

When persistence moves into kernel, product-specific stores should live in the
owning package. Shared low-level file helpers can live under `foundation/fs`.

## Migration Rule

During migration, `tentgent-core` remains the behavior owner. New kernel code
should only add structure or move a coherent bundle with tests. Do not add new
ad hoc path, probe, or manager logic to old core when the matching kernel
package already exists.
