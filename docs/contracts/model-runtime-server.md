# Model Runtime Server

This document defines shared lifecycle behavior for the direct Python model
runtime server.

## Health

`GET /healthz` returns the runtime process snapshot. Rust uses this endpoint to
distinguish ready, closing, and shutdown states for one Python runtime process.

Response fields include:

- `status`: `ok`, `closing`, or `shutdown`
- `pid`
- `server.host`, `server.port`, and optional `server.server_ref`
- `runtime.capability`
- `runtime.model_ref`
- `runtime.resources`
- `tasks`

## Shutdown

`POST /v1/lifecycle/shutdown` requests graceful shutdown of this Python runtime
process.

Behavior:

- the task manager enters `closing`
- new inference requests are rejected
- existing active tasks may finish
- after active tasks finish and the configured closing grace elapses, the
  runtime asks the process host to exit
- resource cleanup still runs through the server lifespan shutdown hook

This endpoint is local to one Python runtime process. Rust daemon process
shutdown remains `POST /v1/daemon/shutdown`; daemon job and server management
remain under `/v1/jobs` and `/v1/servers`.
