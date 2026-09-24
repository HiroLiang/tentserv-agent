# Jobs And Results

Use asynchronous jobs for daemon imports, pulls, dataset synthesis/evaluation, media workflows, and LoRA runs. Start the daemon before using these HTTP operations.

## Examples And Common Operations

Start an asynchronous operation through a feature's `/job` or `/jobs` endpoint, then use the returned job id:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS 'http://127.0.0.1:8790/v1/jobs/<job-id>' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X POST 'http://127.0.0.1:8790/v1/jobs/<job-id>/cancel' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE 'http://127.0.0.1:8790/v1/jobs/<job-id>' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Cancel only an active job; delete only a terminal job after collecting needed results. CLI foreground media commands do not create daemon jobs. There is no `tentgent job` command.

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/jobs` | List daemon-managed jobs. |
| `GET` | `/v1/jobs/{job_id}` | Inspect one job. |
| `POST` | `/v1/jobs/{job_id}/cancel` | Cancel an active job when supported. |
| `DELETE` | `/v1/jobs/{job_id}` | Delete a terminal job record and workspace. Active jobs return conflict. |

Jobs are used for detached model/adapter/dataset operations and media
workflows. The daemon manages job workspaces; public APIs do not expose
workspace chunks or spool routes.

Cancellation updates the durable job state to terminal `canceled` for active
jobs and asks the daemon in-flight handle to abort. Already-started blocking
runtime work may continue outside the durable job state, so cancellation is a
best-effort worker interruption rather than a hard process-kill guarantee.
Terminal jobs are no longer cancellable.

Deleting a terminal job removes both the durable job record and the job
workspace when that workspace exists. Active jobs return `409 job_active`.
Daemon shutdown marks active daemon jobs `interrupted` and runs one
retention-aware workspace sweep; fresh interrupted or just-completed
workspaces are retained for inspection and result/recovery behavior.

### Reading Job Responses

Create/inspect/cancel endpoints return a `job` object; list returns `jobs`.
Read `job.job_id` for polling. Status is `queued`, `running`, `succeeded`,
`failed`, `interrupted`, or `canceled`; only the first two are active.

| Field | Meaning |
| --- | --- |
| `stage`, `cancellable` | Current stage and whether cancellation is available. |
| `progress` | Optional byte/file totals, percent, speed, and ETA. |
| `output.tail` | Recent structured output lines. |
| `artifact`, `result_summary` | Managed result reference/path and completion summary when available. |
| `error_summary`, `warning_summary` | Failure or warning information; inspect after terminal status. |
| `workspace`, `timing` | Available result/input state, retention information, and timestamps. |

A successful job can still require a feature-specific file download. Use the
result routes in the originating feature guide. An artifact path identifies
a daemon-host location, not an HTTP download URL. Dataset and LoRA jobs have
different outputs from media jobs.

## Related Guides

[Datasets](./datasets.md) · [LoRA](./training-lora.md) · [Media](./inference/README.md) · [Recovery](./maintenance.md)
