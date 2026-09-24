# Daemon And HTTP Access

Start the daemon when applications need HTTP access to managed resources and workflows. Its default port is `8790`; [model servers](./servers.md) have their own ports and lifecycle.

## Examples And Common Operations

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
tentgent daemon stop
```

`start` and `run --detach` use the same background launch path. Use `tentgent daemon run --host 127.0.0.1 --port 8790` for a foreground process.

### Bearer Authentication

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
tentgent daemon start --host 127.0.0.1 --port 8790
curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Configure the token before launch. Detached daemon children inherit it. `/healthz` remains public; `/v1/*` requires the bearer token when configured. Local model-server proxy children do not inherit the daemon token.

### HTTP Listener Addresses

`tentgent daemon run` runs in the foreground; `daemon start` and
`daemon run --detach` use the same background launch path.

| Parameter | Behavior |
| --- | --- |
| `--host 127.0.0.1` | Default: accept connections from this machine only. |
| `--host 0.0.0.0` | Listen on all IPv4 interfaces. Clients connect to the machine's actual IP, subject to network and firewall rules. |
| `--host <local-IP>` | Listen on that local address. |
| `--port <port>` | Daemon defaults to `8790`. A server without an explicit port scans from `8780`; an explicit server port is fixed. |
| `--home <path>` | Override runtime state location independently of the listener address. |
| `--allow-unsafe-bind` | Daemon-only opt-in to allow non-loopback binding without a daemon token. |

Non-loopback daemon binds require `TENTGENT_DAEMON_TOKEN` unless explicitly
overridden. The token protects daemon `/v1/*`; `/healthz` stays public. It is
separate from provider keys such as `OPENAI_API_KEY`. Some CLI help still says
"future HTTP listener"; these options already control the active listener.

For commands and payloads, see [daemon commands](./daemon.md),
[server commands](./servers.md), and the [HTTP API](./api.md).

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `daemon run/start` | `-H, --home <HOME>` | Optional Tentgent runtime home override for daemon state |
| `daemon run/start` | `-a, --host <HOST>` | Interface for the active HTTP listener; use 127.0.0.1 for loopback. |
| `daemon run/start` | `-p, --port <PORT>` | TCP port for the future HTTP listener |
| `daemon run/start` | `--allow-unsafe-bind` | Allow binding to non-loopback or wildcard hosts without a daemon token |
| `daemon run` | `--detach` | Launch the daemon in background mode and return after readiness checks |
| `daemon status/stop` | `-H, --home <HOME>` | Optional Tentgent runtime home override for daemon state lookup |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/healthz` | Process liveness and service identity. |
| `GET` | `/v1/status` | Daemon status and runtime-home summary. |
| `GET` | `/v1/auth` | Local provider auth presence. Does not reveal secrets. |
| `GET` | `/v1/auth/{provider}` | Provider auth presence for `hf`, `openai`, `anthropic`, or `gemini`. |
| `GET` | `/v1/doctor` | Observational runtime and dependency report. |
| `GET` | `/v1/daemon/logs` | Daemon log metadata. |
| `GET` | `/v1/daemon/logs/stdout?tail_bytes=8192` | Daemon stdout tail. |
| `GET` | `/v1/daemon/logs/stderr?tail_bytes=8192` | Daemon stderr tail. |
| `POST` | `/v1/daemon/shutdown` | Ask the daemon process to shut down. |

```bash
curl -sS http://127.0.0.1:8790/v1/daemon/logs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS 'http://127.0.0.1:8790/v1/daemon/logs/stderr?tail_bytes=4096' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Log endpoints expose fixed daemon stdout/stderr paths. `POST /v1/daemon/shutdown` requires a configured bearer token even on loopback. Shutdown marks active daemon jobs `interrupted`, performs a retention-aware workspace sweep, and stops only the daemon process. Independently running model servers remain available.

## Related Guides

[HTTP index](./api.md) · [Models](./models.md) · [Datasets](./datasets.md) · [Jobs](./jobs.md) · [Servers](./servers.md)
