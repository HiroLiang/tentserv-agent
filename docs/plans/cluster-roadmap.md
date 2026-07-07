# Cluster Roadmap

Status: active focused sub-roadmap under
[v1.x-roadmap.md](./v1.x-roadmap.md). GitHub issues `#113`-`#118`
track the selected `v1.1.0` cluster slices.

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
| 1. Capability And Resource State Safety | Define how models, adapters, capability metadata, runtime profile references, and cluster route bindings are written, read back, and protected. Establish default delete/remove behavior before cluster routing exists. | A bound model or adapter cannot be deleted or removed by accident; list/inspect reflects writes immediately; explicit unbind or force behavior is documented and tested; current server specs and cluster route bindings use the same protection rule. |
| 2. Cluster Definition And Validation | Add the first cluster shape and internal route-target validation rules without starting a multi-model server yet. A cluster can name routes such as `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` and bind each route to a model/provider reference. | Create/update/list/inspect can show the cluster definition; invalid route names, missing models, missing capabilities, and invalid runtime profile references fail with clear errors; no request routing is required yet. |
| 3. Route Readiness And Diagnostics | Group compatibility proof, tuple-aware model/LoRA checks, runtime profile visibility, and route-level next actions into the cluster inspection path. | Each configured route reports capability, backend/runtime profile, support status, proof state, and next action; one failed or stale route is visible without hiding the rest of the cluster; `doctor` or inspect output can point to the route that needs action. |
| 4. Native Local Routing MVP | Start the first useful cluster runtime path for native local `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` routes. Keep provider-compatible multimodal, tools, and automatic context assembly out of scope. | Requests through the cluster reach the configured local route; unsupported or missing routes fail predictably; route failures are scoped to the route; existing direct single-model server behavior remains unchanged. |
| 5. Runtime Ownership And Shutdown Safety | Add the runtime ownership rules needed once a cluster can run multiple routes. This covers active route ownership, cancellation, shutdown, and cleanup boundaries. | Active cluster routes keep their model/runtime resources from being removed underneath them; shutdown and cancellation release ownership cleanly; cleanup does not delete retained artifacts or bound resources that still have an active owner. |

## v1.1 Issue Drafts

Use this table when creating the initial `v1.1.0` GitHub issues from a clean
session. Keep the parent issue as a tracking issue, then create one sub-issue
per slice after the issue order is accepted.

| Order | Issue Title | Description | Labels |
| --- | --- | --- | --- |
| Parent | `v1.1.0 Cluster MVP` | Track the first `v1.1.0` feature milestone for named clusters. The milestone should deliver a small local multi-route foundation without automatic multimodal context assembly, provider tool orchestration, shared registries, or conversion automation. | `enhancement`, `type:tracking`, `area:roadmap` |
| 1 | `Define Cluster Capability And Resource State Safety` | Define how models, adapters, capability metadata, runtime profile references, server specs, and cluster route bindings are written, read back, and protected. The default behavior should reject accidental deletion of bound resources and require an explicit unbind or separately designed force behavior. | `enhancement`, `type:implementation`, `area:gating`, `area:model-support` |
| 2 | `Add Cluster Definition And Validation` | Add the first cluster definition shape and validation path without request routing. Route targets are internal cluster fields rather than independent public refs. Clusters may be partial, but declared routes must validate route names, model/provider references, capabilities, and runtime profile references before they are stored. | `enhancement`, `type:implementation`, `area:api`, `area:gating` |
| 3 | `Add Cluster Route Readiness Diagnostics` | Show per-route readiness in cluster inspection, including capability, backend/runtime profile, support status, proof state, stale/failed reason, and next action. One failed or stale route must stay visible without hiding the rest of the cluster. | `enhancement`, `type:implementation`, `area:diagnostics`, `area:model-support`, `area:runtime-profile` |
| 4 | `Implement Native Local Cluster Routing MVP` | Route native local `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` requests through the configured cluster routes. Missing, unsupported, or unverified routes should fail with explicit route-scoped errors, and existing direct single-model server behavior must remain unchanged. | `enhancement`, `type:implementation`, `area:api`, `area:gating` |
| 5 | `Add Cluster Runtime Ownership And Shutdown Safety` | Add the runtime ownership rules needed after clusters can run multiple routes. Active cluster routes should protect bound model/runtime resources from unsafe removal, and cancellation, shutdown, and cleanup should release ownership predictably. | `enhancement`, `type:implementation`, `area:gating`, `area:runtime-profile` |

Do not add a SQLite migration issue as a standalone `v1.1.0` slice by default.
If indexed storage is needed, include only the minimum state-family migration
required by the selected slice and document why the file-backed state is no
longer enough.

## Slice Grouping Notes

- Slice 1 is first because cluster routing should not be built on top of
  ambiguous model, adapter, capability, or binding state.
- Durable compatibility proof, tuple-aware LoRA checks, and runtime failure
  evidence are grouped into Slice 3 unless Slice 2 validation proves that part
  of the proof model must exist earlier.
- Minimal resource gates appear in Slice 1 for delete/remove protection and in
  Slice 5 for active runtime ownership. They should not become a broad scheduler
  before the routing MVP exists.
- Automatic multimodal context assembly, provider tool orchestration, OpenShell
  broker auth, cloud rerank, shared registries, and conversion automation remain
  later cluster extensions until these five slices are understood.

## Active #115 Direction

The accepted direction for `#115` is Plan B:

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
| 4. Native Local Routing MVP | Requests through the cluster route to configured local `chat`, `embedding`, `rerank`, `audio-transcription`, and `vision-chat` handlers; unconfigured routes return missing-route or unsupported errors; direct single-model server behavior remains unchanged. |
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
