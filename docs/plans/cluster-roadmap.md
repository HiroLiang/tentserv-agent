# Cluster Roadmap

Status: final slice `#118`, including remediation findings `R1`-`R19`, is
implemented and passes the local full-suite and Rust 1.81 verification
matrices. Native Windows CI rerun remains pending under
[v1.x-roadmap.md](./v1.x-roadmap.md), so parent tracking issue `#113` closeout
and archival remain blocked.

This plan keeps the selected `v1.1.0` cluster slices aligned while leaving
later cluster-related candidates visible without turning every related
foundation into a committed `v1.1.0` requirement.

## Summary

A cluster is the user-facing named local routing object. It owns internal route
targets that map capability requests to configured models or providers. Users
should operate on one cluster ref such as `local-assistant`, not on a separate
target ref. The target/route-target terminology is an
implementation detail inside a cluster definition.

Instead of starting one isolated server for one model, users can define one
cluster and let Tentgent route `chat`, `embedding`, `rerank`,
`audio-transcription`, `vision-chat`, and later media or tool workflows through
the matching configured route.

The first cluster milestone should be small and should start with state safety:
bound resources must be visible, protected from accidental removal, and
inspectable before Tentgent adds multi-route serving behavior.

## Implementation Status

| Issue | Slice | Status |
| --- | --- | --- |
| `#114` | Capability and resource state safety contract | completed |
| `#115` | Cluster definition and validation | completed |
| `#116` | Route readiness and diagnostics | completed |
| `#117` | Native local cluster routing MVP | completed |
| `#118` | Runtime ownership and shutdown safety | all remediation and tracking alignment pass; Windows CI rerun pending |

The next action is to pass the remaining verification gates in
[cluster-runtime-ownership-remediation.md](./cluster-runtime-ownership-remediation.md),
then repeat the combined cluster MVP review. Parent `#113` closeout and plan
archival must not begin before those completion gates pass. No sixth cluster
implementation slice is implied by this remediation.

## Candidate Groups

### Core Cluster MVP

These items are the likely first group to discuss for `v1.1.0`:

- cluster schema and internal route-target validation rules
- read-only cluster inspection command and API
- native local routing for `chat`, `embedding`, `rerank`,
  `audio-transcription`, and `vision-chat`
- clear unsupported errors when a configured route is missing, unsupported, or
  not verified enough for the requested workflow

### Compatibility Foundations

These items may be part of the MVP or may need closely related issues before
cluster start behavior can be trusted:

- durable compatibility proof foundation
- tuple-aware model and LoRA compatibility gates
- consistent inspect and doctor output for each configured route
- runtime failure evidence that remains visible after a failed route attempt

### Runtime Safety Foundations

These items protect multi-model serving from unsafe local state changes:

- minimal resource gates for model mutation, adapter writes, warm server
  ownership, and GPU/CPU-sensitive execution
- cancellation and shutdown behavior that respects active route ownership
- bounded route startup behavior so one bad route does not hide the state of the
  whole cluster

### Later Cluster Extensions

These items should stay out of the first cluster MVP unless a later planning
pass intentionally promotes them:

- automatic multimodal context assembly
- file, image, audio, or video pre-processing into chat context
- provider-compatible tool or function-call orchestration
- OpenShell-managed provider gateway or broker auth
- cloud rerank provider adoption
- shared compatibility registry integration
- conversion automation or generated model metadata

## Design Rules

- Clusters may be partial. A cluster only promises the routes it declares.
  Unconfigured capabilities should fail with clear missing-route or unsupported
  errors instead of making the whole cluster invalid.
- The first MVP supports one configured route per capability. Multiple route
  variants for the same capability, such as `chat.fast` and `chat.quality`, are
  deferred until the basic cluster route model is proven.
- User-facing commands and REST routes should use `cluster`, not standalone
  `target`. Route targets are internal cluster fields and should not have
  independent public refs in the first MVP.
- The canonical stored definition format is TOML under
  `TENTGENT_HOME/clusters/<cluster_ref>/cluster.toml`. CLI import may accept a
  TOML definition file. REST should use JSON request/response bodies and the
  daemon should normalize stored state to the same TOML schema.
- Cluster refs should use a conservative lowercase ASCII slug format:
  `[a-z0-9][a-z0-9._-]{0,63}`. Slashes, whitespace, uppercase letters, hidden
  path segments, and shell-sensitive characters are invalid.
- `tentgent cluster apply <CLUSTER_TOML>` replaces the stored cluster
  definition atomically. Removing a route from the TOML unbinds that route in
  stored state. Later route-level commands may add, remove, or replace one
  route explicitly, but `apply` is a full-definition replace.
- `tentgent cluster validate <CLUSTER_TOML>` should parse and validate the
  definition without writing it.
- Definition file reads should be conservative. The CLI should only read the
  explicit file path supplied by the user, must not support include/import
  directives, must reject directories and special files, and should reject
  obvious secret-bearing locations such as SSH, GnuPG, macOS Keychain, and
  Tentgent auth-secret paths unless the user passes an explicit `--force`.
  `--force` may bypass the source-location warning, but it must not bypass
  schema validation, reference validation, or special-file rejection.
- Daemon REST CRUD should use cluster names, not target names:
  `GET /v1/clusters`, `PUT /v1/clusters/{cluster_ref}`,
  `GET /v1/clusters/{cluster_ref}`, and
  `DELETE /v1/clusters/{cluster_ref}`.
- Cluster definitions do not make `model + adapter` a fixed route target.
  Chat routes preserve the existing server-chat request-time `adapter_ref`
  behavior, so adapters remain dynamically selectable per request.
- The default delete/remove behavior is protective. A model, adapter, runtime
  profile, or cluster route binding that is referenced by an active server spec
  or cluster should reject normal deletion until the user explicitly unbinds it.
  Force-style deletion should be a separately designed behavior, not the default
  path. The structured blocker and guard-validator model is defined in
  [resource-blockers.md](../contracts/resource-blockers.md).
- SQLite, or an equivalent indexed local metadata layer, is likely the right
  long-term backend for cluster route bindings, proof records, and resource ownership
  queries. Storage should move incrementally by state family, not through one
  broad migration. The first slice should define state and delete-protection
  rules before forcing any storage backend change.
- Durable compatibility proof storage and tuple-aware model/LoRA gates should
  be defined as route-readiness concepts for cluster planning. Their storage and
  enforcement can be implemented slice by slice as the cluster requires them.

## Proposed Slices

These slices are intentionally broad. Each one should be large enough to verify
as a user-visible or operator-visible milestone, but small enough to review
without mixing unrelated cluster features.

| Slice | Scope | Completion Checks |
| --- | --- | --- |
| 1. Capability And Resource State Safety | Define how models, adapters, capability metadata, runtime profile references, and cluster route bindings are written, read back, and protected. Establish default delete/remove behavior before cluster routing exists. | A bound model or adapter cannot be deleted or removed by accident; list/inspect reflects writes immediately; blockers identify the references that must be stopped or removed; current server specs and cluster route bindings use the same protection rule. |
| 2. Cluster Definition And Validation | Add the first cluster shape and internal route-target validation rules without starting a multi-model server yet. A cluster can name routes such as `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` and bind each route to a model/provider reference. | Create/update/list/inspect can show the cluster definition; invalid route names, missing models, missing capabilities, and invalid runtime profile references fail with clear errors; no request routing is required yet. |
| 3. Route Readiness And Diagnostics | Group existing model support evidence, runtime profile visibility, and route-level next actions into the cluster inspection path. Tuple-aware LoRA gates remain later 1.x work. | Each configured route reports capability, backend/runtime profile, support status, proof state, and next action; one failed or stale route is visible without hiding the rest of the cluster; `doctor` or inspect output can point to the route that needs action. |
| 4. Cluster Server Routing MVP | Start the first useful cluster server path for native local `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` routes. Keep provider-compatible multimodal, tools, and automatic context assembly out of scope. | Requests sent to a cluster server reach the configured local route; unsupported or missing routes fail predictably; route failures are scoped to the route; existing direct single-model server behavior remains unchanged. |
| 5. Runtime Ownership And Shutdown Safety | Add the runtime ownership rules needed once a cluster can run multiple routes. This covers active route ownership, cancellation, shutdown, and cleanup boundaries. | Active cluster routes keep their model/runtime resources from being removed underneath them; shutdown and cancellation release ownership cleanly; cleanup does not delete retained artifacts or bound resources that still have an active owner. |

## v1.1 Issue Drafts

Use this table when creating the initial `v1.1.0` GitHub issues from a clean
session. Keep the parent issue as a tracking issue, then create one sub-issue
per slice after the issue order is accepted.

| Order | Issue Title | Description | Labels |
| --- | --- | --- | --- |
| Parent | `v1.1.0 Cluster MVP` | Track the first `v1.1.0` feature milestone for named clusters. The milestone should deliver a small local multi-route foundation without automatic multimodal context assembly, provider tool orchestration, shared registries, or conversion automation. | `enhancement`, `type:tracking`, `area:roadmap` |
| 1 | `Define Cluster Capability And Resource State Safety` | Define how models, adapters, capability metadata, runtime profile references, server specs, and cluster route bindings are written, read back, and protected. The default behavior rejects accidental deletion of bound resources and reports the references that must be stopped or removed first; force does not bypass active ownership. | `enhancement`, `type:implementation`, `area:gating`, `area:model-support` |
| 2 | `Add Cluster Definition And Validation` | Add the first cluster definition shape and validation path without request routing. Route targets are internal cluster fields rather than independent public refs. Clusters may be partial, but declared routes must validate route names, model/provider references, capabilities, and runtime profile references before they are stored. | `enhancement`, `type:implementation`, `area:api`, `area:gating` |
| 3 | `Add Cluster Route Readiness Diagnostics` | Show per-route readiness in cluster inspection, including capability, backend/runtime profile, support status, proof state, stale/failed reason, and next action. One failed or stale route must stay visible without hiding the rest of the cluster. | `enhancement`, `type:implementation`, `area:diagnostics`, `area:model-support`, `area:runtime-profile` |
| 4 | `Implement Native Local Cluster Routing MVP` | Add the first cluster server path so native local `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` HTTP requests sent to that server are dispatched through configured cluster routes. Missing, unsupported, or unverified routes should fail with explicit route-scoped errors, and existing direct single-model server behavior must remain unchanged. | `enhancement`, `type:implementation`, `area:api`, `area:gating` |
| 5 | `Add Cluster Runtime Ownership And Shutdown Safety` | Add the runtime ownership rules needed after clusters can run multiple routes. Active cluster routes should protect bound model/runtime resources from unsafe removal, and cancellation, shutdown, and cleanup should release ownership predictably. | `enhancement`, `type:implementation`, `area:gating`, `area:runtime-profile` |

Do not add a SQLite migration issue as a standalone `v1.1.0` slice by default.
If indexed storage is needed, include only the minimum state-family migration
required by the selected slice and document why the file-backed state is no
longer enough.

## Slice Grouping Notes

- Slice 1 is first because cluster routing should not be built on top of
  ambiguous model, adapter, capability, or binding state.
- Existing compatibility proof and runtime failure evidence feed Slice 3.
  Durable proof migration and tuple-aware LoRA enforcement remain separate
  later 1.x work.
- Minimal resource gates appear in Slice 1 for delete/remove protection and in
  Slice 5 for active runtime ownership. They should not become a broad scheduler
  before the routing MVP exists.
- Automatic multimodal context assembly, provider tool orchestration, OpenShell
  broker auth, cloud rerank, shared registries, and conversion automation remain
  later cluster extensions until these five slices are understood.

## Completed #115 Record

`#115` implemented Plan B:

- `cluster` is the public object and public command/API name.
- `target` or `route target` is an internal structure inside a cluster
  definition.
- Users should refer to one `cluster_ref`; they should not manage a separate
  `target_ref`.
- The canonical persisted definition is TOML:

```text
TENTGENT_HOME/
└── clusters/
    └── <cluster_ref>/
        └── cluster.toml
```

- CLI definition import should use a TOML file, for example
  `tentgent cluster apply <CLUSTER_TOML>`. The file is a declarative cluster
  definition, not a Dockerfile-style build file and not a launch command.
- CLI validation should be available as
  `tentgent cluster validate <CLUSTER_TOML>`.
- `apply` is full replacement, not a route patch. Route-level insertion or
  removal should be modeled as later explicit commands if needed.
- REST CRUD for cluster definitions is part of `#115` and remains
  definition-only: `GET /v1/clusters`, `PUT /v1/clusters/{cluster_ref}`,
  `GET /v1/clusters/{cluster_ref}`, and
  `DELETE /v1/clusters/{cluster_ref}`.
- Route keys should be based on canonical capabilities. The `v1.1.0` MVP must
  cover `chat`, `embedding`, `rerank`, `audio-transcription`, and
  `vision-chat` as first-class cluster routes.
- Adapter selection remains dynamic. Cluster definitions should not store a
  fixed `model + adapter` target in `v1.1.0`; request-time `adapter_ref`
  validation stays in the existing server-chat path.
- YAML is deferred. TOML is the only planned definition file format for the
  first cluster definition slice. A future input-format expansion may add YAML
  as an import format while keeping canonical stored state as TOML.

## Adapter Binding Decision

Cluster routes should preserve Tentgent's native dynamic adapter behavior.

A cluster `chat` route binds the base model and runtime route. Requests may
still pass `adapter_ref` through the existing server-chat request-time adapter
selection path. The cluster should not turn `model + adapter` into a separate
fixed route target in `v1.1.0`, and later cluster work should not require that
shape unless a separate route-variant feature is explicitly designed.

This keeps adapter switching as a first-class runtime feature instead of
pre-expanding one cluster route per adapter. `#115` should validate only the
base route references needed to store the cluster safely. Adapter existence,
compatibility, backend support, and execution support continue to be validated
by the existing request-time server adapter path.

## Completed #116 Record

`#116` implemented diagnostics-only route readiness.

`tentgent cluster apply <CLUSTER_TOML>` and REST cluster `PUT` should continue
to answer only whether the definition is valid and stored. They should not run
readiness checks, start runtimes, verify proofs, read secrets, or mutate proof
state. Detailed route readiness belongs to `tentgent cluster inspect
<cluster_ref>` and REST cluster inspect responses after the definition is
stored.

Cluster readiness should be computed from existing state at inspect/doctor
time. It should not be written back into `cluster.toml`, and it should not add
a separate cluster readiness cache. The resolver should read cluster
definition, model metadata, capability metadata, runtime profile selection,
stored model support proofs, provider support metadata, and non-invasive auth
metadata.

`cluster inspect` should show the cluster definition and route status together:

- first a compact cluster summary with `cluster_ref`, `schema_version`,
  aggregate readiness, configured route list, and definition path;
- then a route table where each configured route shows route key, target kind,
  target, capability, runtime profile, readiness status, and next action;
- then a problem/details section only for routes that need attention.

Partial clusters are valid. The main route table should list configured routes.
Unconfigured capabilities may appear in a compact summary or reminder, but they
should not be treated as warnings unless the user asks to run a missing route in
a later routing slice.

Runtime profile handling should be explicit but non-mutating. If a local route
omits a runtime profile and the existing server/runtime-profile rules can infer
one, inspect output should show both the configured value and the effective
inferred value. The resolver must not rewrite the TOML automatically. It may
attach a `runtime-profile-inferred` flag and a next action that suggests adding
the inferred profile to the cluster definition for reproducibility.

Provider route readiness must not trigger Keychain prompts, network calls, or
secret reads. It may read auth preferences, environment/file presence, and
cached metadata that are already safe for observational diagnostics. Missing
provider auth should produce an auth-related warning and a next action, but
invalid cloud credentials should not be proven by this slice.

Auth checks should use the existing auth use-case separation instead of
open-coded probing. `AuthStatusUseCase` is the non-secret status boundary and
should be used for cluster readiness with keychain presence probing disabled by
default. `AuthSecretResolverUseCase` and `AuthSecretValidationUseCase` are for
secret reads, Keychain unlock behavior, and network validation; cluster
readiness must not call them. Cluster readiness code should not receive secret
material. If the existing status path reads environment or file secret material
internally only to determine presence, keep that containment inside the auth
boundary or introduce a presence-only auth summary adapter. If cluster
readiness, CLI doctor, REST doctor, or auth status need the same auth summary
shape, `#116` should consolidate that summary behind a shared kernel helper or
use-case adapter rather than duplicating per-entrypoint logic.

CLI `doctor` and REST `/v1/doctor` should be aligned. Both should include a
cluster readiness summary, while detailed per-route state stays in cluster
inspect. Doctor checks should be short and should point to
`tentgent cluster inspect <cluster_ref>` or the corresponding REST cluster
inspect resource for route-level detail.

Doctor should add a first-class `cluster` category instead of folding cluster
readiness into `capability` or `runtime`. CLI and REST doctor output should use
that category consistently so users can distinguish environment capability
checks from cluster route diagnostics.

REST response shapes should be structured and additive. Cluster inspect should
include an aggregate `readiness` object and per-route `readiness` plus
`next_actions`. Doctor checks should keep the existing fields and may add
structured fields such as:

- `category`
- `description`
- `flags`
- `details`
- `next_actions[].code`

If a new structured field overlaps an older field, keep the older field for
backward compatibility and mark it deprecated in the relevant contract or DTO
documentation. For example, if `description` becomes the canonical readable
message, `detail` should remain populated as a legacy alias until a later major
release removes it.

Aggregate cluster readiness should use the first-version vocabulary:

- `ready`: every configured route is ready enough to use;
- `partial`: at least one configured route is ready and at least one route
  needs attention;
- `blocked`: every configured route needs attention or a configured route has a
  blocking definition/readiness error;
- `unknown`: readiness could not be computed because supporting state could not
  be read.

The first route-readiness next-action codes should be defined in `#116` rather
than deferred to a later slice. Planned codes are:

- `inspect-cluster`
- `inspect-model`
- `verify-model-capability`
- `clear-model-proof`
- `set-provider-auth`
- `update-cluster-definition`
- `choose-supported-route-target`

Kernel readiness results should prefer structured action codes and route state.
CLI commands can render those actions as copyable commands, while REST can
return the action code, label, command, and description.

`#116` implementation should keep CLI rendering thin. The readiness resolver,
route summaries, aggregate status, flags, details, and action codes should live
in kernel-owned types so CLI cluster inspect, CLI doctor, REST cluster inspect,
and REST doctor all consume the same result model.

Implementation order for `#116`:

1. Define kernel route-readiness domain types and action-code vocabulary.
2. Add a read-only cluster-readiness resolver that reuses existing cluster,
   model, support-status, proof, runtime-profile, provider-support, and
   non-secret auth-status boundaries.
3. Extend cluster inspect use cases and REST cluster inspect DTOs with
   aggregate and per-route readiness data.
4. Render CLI `cluster inspect` as summary, route table, and problem details.
5. Add cluster-category doctor summary checks to both CLI doctor and REST
   `/v1/doctor`.
6. Update cluster, HTTP daemon, doctor, and user command docs for the new
   structured fields and any deprecated legacy aliases.
7. Test local route states, provider auth/readiness states, doctor summary
   alignment, and REST response shape.

Implementation outcome for `#116`:

- kernel-owned route-readiness status, aggregate status, flags, details, and
  next-action codes;
- read-only cluster readiness inspect/list use cases that reuse existing
  cluster, model, proof, runtime-profile, provider-support, and non-secret auth
  status boundaries;
- CLI `cluster inspect` summary, route table, and problem details;
- REST `GET /v1/clusters/{cluster_ref}` readiness fields while `PUT` remains
  definition-only;
- CLI doctor and REST `/v1/doctor` cluster-category summary checks;
- contract and user docs aligned with the diagnostics-only boundary;
- no cluster request routing, no cluster run/start lifecycle, no readiness
  cache, no proof mutation, no secret read, and no runtime startup.

## #117 Implementation Outcome

`#117` completes Slice 4 with the first experimental cluster server runtime:

- `tentgent cluster run <cluster_ref>` creates a structured cluster server
  target and reuses server refs, specs, process metadata, logs, health,
  foreground/background launch, auto-port, start, stop, and remove behavior;
- daemon `POST /v1/servers` accepts `runtime_kind: "cluster"` plus
  `cluster_ref`, while server responses expose an additive structured target;
- provider-shaped text chat, native chat, embedding, rerank, audio
  transcription, and vision chat endpoint families select their exact cluster
  route without cross-route fallback or caller `model` overrides;
- route execution reuses local model handlers, tuple-aware support gates,
  runtime profiles, the shared Python runtime supervisor, and runtime-execution
  proof evidence;
- partial clusters remain valid, but server startup requires a local
  `routes.chat`; optional route failures stay request-scoped;
- running servers cache the parsed definition and reload changed canonical TOML
  using a definition hash; reload failures never silently use stale targets;
- provider targets return `cluster_route_target_unsupported` and remain later
  orchestration work;
- cluster removal is blocked by any running or stopped server spec that targets
  the cluster, without cascade deletion;
- no SQLite migration, fixed adapter target, automatic fallback, or broad
  provider-compatible multimodal promise was added.

## #118 Implementation And Remediation Record

`#118` has implemented active route claims, profile-aware physical runtime
generations, cross-process transition locks, typed resource guards, safe
drain/block reload, bounded shutdown, managed adapter resolution, ownership
inspect/doctor output, and stale-state reconciliation. A post-implementation
audit found lifecycle, mutation stabilization, platform replacement, lock
metadata, and focused-inspection gaps. The approved design and decisions remain in
[cluster-runtime-ownership-plan.md](./cluster-runtime-ownership-plan.md), with
module boundaries in
[cluster-runtime-coordination-architecture.md](./cluster-runtime-coordination-architecture.md)
and active fixes in
[cluster-runtime-ownership-remediation.md](./cluster-runtime-ownership-remediation.md).

SQLite remains a later isolated state-family migration. Provider cluster target
execution, fixed adapter targets, automatic route fallback, and a general
scheduler remain outside the v1.1 cluster MVP and this remediation.

## GitHub Issue And Branch Alignment

GitHub issue text and branch names were originally created before the Plan B
naming decision. They were aligned to cluster naming on July 7, 2026. Old remote
branches may still exist for GitHub branch-binding cleanup, but new work should
use the aligned branch names below.

| Issue | Previous Title | Aligned Title | Previous Branch | Aligned Branch |
| --- | --- | --- | --- | --- |
| `#113` | `v1.1.0 Cluster / Serving Target MVP` | `v1.1.0 Cluster MVP` | `feature/113-v1.1-cluster-serving-target-mvp` | `feature/113-v1.1-cluster-mvp` |
| `#114` | `Define Serving Target Capability And Resource State Safety` | `Define Cluster Capability And Resource State Safety` | completed before branch realignment | no branch change required unless reopening |
| `#115` | `Add Serving Target Definition And Validation` | `Add Cluster Definition And Validation` | `feature/115-serving-target-definition-validation` | `feature/115-cluster-definition-validation` |
| `#116` | `Add Serving Target Route Readiness Diagnostics` | `Add Cluster Route Readiness Diagnostics` | `feature/116-serving-target-readiness-diagnostics` | `feature/116-cluster-route-readiness-diagnostics` |
| `#117` | `Implement Native Local Serving Target Routing MVP` | `Implement Native Local Cluster Routing MVP` | `feature/117-native-serving-target-routing-mvp` | `feature/117-native-cluster-routing-mvp` |
| `#118` | `Add Serving Target Runtime Ownership And Shutdown Safety` | `Add Cluster Runtime Ownership And Shutdown Safety` | `feature/118-serving-target-runtime-ownership` | `feature/118-cluster-runtime-ownership` |

## Verification Shape

Each slice should produce observable behavior before the next slice depends on
it:

| Slice | Minimum Verification Shape |
| --- | --- |
| 1. Capability And Resource State Safety | Bind a model or adapter; inspect/list reflects the binding immediately; normal deletion of the bound resource is rejected; explicit unbind allows deletion; the behavior is covered for current server specs and future cluster route bindings. |
| 2. Cluster Definition And Validation | Create a partial cluster with only `chat`; inspect shows only configured routes; invalid route names, missing models, missing capabilities, and invalid runtime profile references fail before any request routing exists. |
| 3. Route Readiness And Diagnostics | Inspect shows per-route readiness for ready, unknown, stale, failed, and unsupported states; failure reason and next action are visible; `doctor` summarizes cluster route problems and points to cluster inspect for details. |
| 4. Cluster Server Routing MVP | Requests sent to a cluster server reach configured local `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` routes; unconfigured routes return missing-route or unsupported errors; direct single-model server behavior remains unchanged. |
| 5. Runtime Ownership And Shutdown Safety | Active cluster route ownership blocks unsafe resource removal; cancellation and shutdown release ownership; cleanup skips resources or artifacts still retained by active ownership. |

## Non-Goals For The First Cluster MVP

- Do not add more `v1.1.0` cluster slices unless this roadmap is updated first.
- Do not require the first cluster slice to support every capability in Tentgent.
- Do not imply provider-compatible multimodal behavior before the native routing
  and compatibility foundations are clear.
- Do not hide missing capability, proof, runtime profile, or resource conflicts
  behind fallback routing.

## Follow-Up Checklist

Use this checklist when refining the remaining cluster issues:

- Which capability set is required for the first useful cluster?
- Is inspection required before start/run behavior?
- Which compatibility proof state is required to allow a route?
- Which failures should block the whole cluster versus only one route?
- Which resource conflicts must be guarded in the first release?
- Which items belong in `v1.1.0`, and which should remain later `v1.x` work?
