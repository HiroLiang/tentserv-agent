# Cluster Serving Target Roadmap

Status: focused sub-roadmap under [v1.x-roadmap.md](./v1.x-roadmap.md).

This plan narrows the `v1.x` cluster direction before individual issues are
selected and ordered. It should collect cluster-related candidates in one place
without turning every related foundation into a committed `v1.1.0` requirement.

## Summary

A cluster, also called a serving target, is a named local target that can route
different capability requests to different configured models or providers.
Instead of starting one isolated server for one model, users can define one
target such as `local-assistant` and let Tentgent route `chat`, `embedding`,
`rerank`, and later media or tool workflows through the matching configured
route.

The first cluster milestone should be small and should start with state safety:
bound resources must be visible, protected from accidental removal, and
inspectable before Tentgent adds multi-route serving behavior.

## Candidate Groups

### Core Cluster MVP

These items are the likely first group to discuss for `v1.1.0`:

- serving target schema and validation rules
- read-only target inspection command and API
- native local routing for `chat`, `embedding`, and `rerank`
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
  whole target

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

- Targets may be partial. A target only promises the routes it declares.
  Unconfigured capabilities should fail with clear missing-route or unsupported
  errors instead of making the whole target invalid.
- The first MVP supports one configured route per capability. Multiple route
  variants for the same capability, such as `chat.fast` and `chat.quality`, are
  deferred until the basic target model is proven.
- The default delete/remove behavior is protective. A model, adapter, runtime
  profile, or future target binding that is referenced by an active server spec
  or serving target should reject normal deletion until the user explicitly
  unbinds it. Force-style deletion should be a separately designed behavior, not
  the default path.
- SQLite, or an equivalent indexed local metadata layer, is likely the right
  long-term backend for target bindings, proof records, and resource ownership
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
| 1. Capability And Resource State Safety | Define how models, adapters, capability metadata, runtime profile references, and future target bindings are written, read back, and protected. Establish default delete/remove behavior before cluster routing exists. | A bound model or adapter cannot be deleted or removed by accident; list/inspect reflects writes immediately; explicit unbind or force behavior is documented and tested; current server specs and future target bindings use the same protection rule. |
| 2. Serving Target Definition And Validation | Add the first serving target shape and validation rules without starting a multi-model server yet. A target can name routes such as `chat`, `embedding`, and `rerank` and bind each route to a model/provider reference. | Create/update/list/inspect can show the target definition; invalid route names, missing models, missing capabilities, and invalid runtime profile references fail with clear errors; no request routing is required yet. |
| 3. Route Readiness And Diagnostics | Group compatibility proof, tuple-aware model/LoRA checks, runtime profile visibility, and route-level next actions into the target inspection path. | Each configured route reports capability, backend/runtime profile, support status, proof state, and next action; one failed or stale route is visible without hiding the rest of the target; `doctor` or inspect output can point to the route that needs action. |
| 4. Native Local Routing MVP | Start the first useful target runtime path for native local `chat`, `embedding`, and `rerank` routes. Keep provider-compatible multimodal, tools, and automatic context assembly out of scope. | Requests through the target reach the configured local route; unsupported or missing routes fail predictably; route failures are scoped to the route; existing direct single-model server behavior remains unchanged. |
| 5. Runtime Ownership And Shutdown Safety | Add the runtime ownership rules needed once a target can run multiple routes. This covers active route ownership, cancellation, shutdown, and cleanup boundaries. | Active target routes keep their model/runtime resources from being removed underneath them; shutdown and cancellation release ownership cleanly; cleanup does not delete retained artifacts or bound resources that still have an active owner. |

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

## Verification Shape

Each slice should produce observable behavior before the next slice depends on
it:

| Slice | Minimum Verification Shape |
| --- | --- |
| 1. Capability And Resource State Safety | Bind a model or adapter; inspect/list reflects the binding immediately; normal deletion of the bound resource is rejected; explicit unbind allows deletion; the behavior is covered for current server specs and future target bindings. |
| 2. Serving Target Definition And Validation | Create a partial target with only `chat`; inspect shows only configured routes; invalid route names, missing models, missing capabilities, and invalid runtime profile references fail before any request routing exists. |
| 3. Route Readiness And Diagnostics | Inspect shows per-route readiness for ready, unknown, stale, failed, and unsupported states; failure reason and next action are visible; `doctor` summarizes target route problems and points to target inspect for details. |
| 4. Native Local Routing MVP | Requests through the target route to configured local `chat`, `embedding`, and `rerank` handlers; unconfigured routes return missing-route or unsupported errors; direct single-model server behavior remains unchanged. |
| 5. Runtime Ownership And Shutdown Safety | Active target route ownership blocks unsafe resource removal; cancellation and shutdown release ownership; cleanup skips resources or artifacts still retained by active ownership. |

## Non-Goals For First Discussion

- Do not turn these slices into GitHub issues until the slice boundaries are
  accepted.
- Do not require the first cluster slice to support every capability in Tentgent.
- Do not imply provider-compatible multimodal behavior before the native routing
  and compatibility foundations are clear.
- Do not hide missing capability, proof, runtime profile, or resource conflicts
  behind fallback routing.

## Discussion Checklist

Use this checklist to decide which candidates become cluster issues:

- Which capability set is required for the first useful target?
- Is inspection required before start/run behavior?
- Which compatibility proof state is required to allow a route?
- Which failures should block the whole target versus only one route?
- Which resource conflicts must be guarded in the first release?
- Which items belong in `v1.1.0`, and which should remain later `v1.x` work?
