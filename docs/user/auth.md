# Provider Authentication

Configure only the providers you use. Public Hugging Face models may work without a token; gated repositories require access granted by the publisher. Provider keys and the daemon bearer token serve different purposes.

## Examples And Common Operations

Check all provider keys:

```bash
tentgent auth status
```

Set provider keys:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
tentgent auth gemini set
```

Inspect or remove one provider key:

```bash
tentgent auth hf
tentgent auth hf rm
tentgent auth openai
tentgent auth openai rm
tentgent auth anthropic
tentgent auth anthropic rm
tentgent auth gemini
tentgent auth gemini rm
```

Inspect or set provider auth source modes:

```bash
tentgent auth mode
tentgent auth mode openai
tentgent auth mode openai auto
tentgent auth mode openai env
tentgent auth mode gemini file --path ~/.config/tentgent/provider.env
tentgent auth mode anthropic none
```

Available modes:

- `auto`: request/prompt, `.env` / process env, process cache, Keychain, then
  none.
- `keychain`: only Tentgent-managed system secret storage.
- `file`: only the explicit env file configured with `--path`.
- `env`: only process environment variables.
- `none`: disable local provider auth resolution.

Use `env` when OpenShell or another launcher injects standard variables such as
`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, or `HF_TOKEN`.

`file` mode reads the same variable names from the configured env file:

```dotenv
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GEMINI_API_KEY=...
```

`rm` removes the Tentgent-managed stored key. An environment variable or explicit env file can still provide credentials; use mode `none` to disable resolution.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `auth mode` | `--path <PATH>` | Explicit env file path for file mode |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/auth` | Local provider auth presence. Does not reveal secrets. |
| `GET` | `/v1/auth/{provider}` | Provider auth presence for `hf`, `openai`, `anthropic`, or `gemini`. |

Provider key set/remove stays local-only through the CLI. Read status through HTTP:

```bash
curl -sS http://127.0.0.1:8790/v1/auth \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth/openai \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Daemon auth status reports local env/keychain presence only. It does not print
secrets and does not call provider validation endpoints.

Auth status returns `providers` for the list endpoint or `provider` for one
provider. Each item includes `source_mode`, `env_present`, `keychain_present`,
`effective_source`, and a `validation` object. These are presence/diagnostic
fields, not credential values or a live provider validity check.

## Related Guides

[Runtime and Keychain prompts](./runtime.md#keychain-prompts) · [Provider integrations](./provider-compatible-examples.md)
