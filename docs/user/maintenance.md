# Diagnostics And Maintenance

Use `doctor` for installation, model support, and Cluster readiness; use `runtime reconcile` for stale ownership and `store gc` for abandoned imports. Stop the affected workload before applying a repair.

## Examples And Common Operations

```bash
tentgent doctor
tentgent runtime reconcile
tentgent store gc
```

These commands report existing state. Read the next action before applying a repair.
For packaged installs, use `tentgent runtime bootstrap` to install dependencies.
`doctor --fix` is the developer bootstrap path and requires `uv`.

### Store Staging Cleanup

Interrupted model, adapter, or dataset imports can leave partial files under
managed staging directories before Tentgent has computed a content hash and
installed a canonical `store/<ref>` entry.

Inspect abandoned staging directories without deleting anything:

```bash
tentgent store gc
```

Delete the listed staging directories:

```bash
tentgent store gc --apply
```

This command only removes direct children of `models/staging`,
`adapters/staging`, and `datasets/staging`. It does not remove hashed model,
adapter, or dataset content under `store/<ref>`; use the specific `model rm`,
`adapter rm`, or `dataset rm` commands for canonical objects.

### Stale Runtime And Job State

Daemon job workspaces are temporary runtime state under `TENTGENT_HOME`. They
may be retained after success, failure, cancellation, interruption, or daemon
shutdown so result routes and inspection still have stable files to read.
Deleting a terminal job through the daemon job API removes both the durable job
record and its workspace:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs
curl -X DELETE http://127.0.0.1:8790/v1/jobs/<job-id>
```

Active jobs cannot be deleted. Cancel them first when the job is still active:

```bash
curl -X POST http://127.0.0.1:8790/v1/jobs/<job-id>/cancel
```

Cancellation always updates the durable job state, but already-started blocking
runtime work may take time to stop. If daemon shutdown or restart leaves an
active job in doubt, restart the daemon and inspect `/v1/jobs`; previously
queued or running daemon jobs are recorded as `interrupted` instead of being
silently reused.

Local model-bound server processes are separate from daemon jobs. Use server
inspection and stop commands for stale server runtime state:

```bash
tentgent server ps
tentgent server inspect <server-ref>
tentgent server stop <server-ref>
```

Cluster routes and shared Python runtimes also retain durable ownership records
so another process cannot remove their model or capability during a lifecycle
transition. Inspect recovery actions without changing state:

```bash
tentgent runtime reconcile
```

`tentgent cluster inspect <cluster-ref>` and
`tentgent server inspect <server-ref>` show safe ownership details scoped to
that object. They omit process ids, process tokens, local ownership paths, and
internal generation ids. Doctor remains a compact global summary.

Stop the owner named by the report before applying recovery. Then use:

```bash
tentgent runtime reconcile --apply
```

Malformed state is quarantined only when no live process can own it. A later
`tentgent runtime reconcile --apply --purge-quarantine` removes records that
were quarantined before that invocation. Unreadable or unverifiable state
remains blocked instead of being deleted.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `doctor` | `-f, --fix` | Developer bootstrap: create or sync the managed Python environment with uv before checking health |
| `runtime reconcile` | `-H, --home <HOME>` | Optional Tentgent runtime home override |
| `runtime reconcile` | `--apply` | Apply proven-stale repairs. Without this flag the command is read-only |
| `runtime reconcile` | `--purge-quarantine` | Purge records quarantined before this invocation. Requires --apply |
| `store gc` | `-H, --home <HOME>` | Runtime home override for store lookup |
| `store gc` | `--apply` | Delete listed staging directories. Without this flag, only prints what would be removed |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

`GET /v1/doctor` returns the observational health report. Repairs remain CLI operations. For asynchronous work, use the [job lifecycle guide](./jobs.md).

## Related Guides

[Runtime setup](./runtime.md) · [Model diagnostics](./models.md) · [Cluster recovery](./clusters.md)
