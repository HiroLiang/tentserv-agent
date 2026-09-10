# Sessions And Conversation Context

A session stores bounded local text conversation context. CLI `chat --session` loads and records it automatically. Native daemon and direct-server chat remain stateless; HTTP applications manage session records explicitly.

## Examples And Common Operations

Create and inspect local sessions from the CLI:

```bash
tentgent session create --title "Planning" --tag draft
tentgent session ls
tentgent session inspect <session-ref>
tentgent session append <session-ref> --role user --content "Hello"
tentgent session append <session-ref> --role user --content "Hello" --compaction-server <server-ref>
tentgent session compact <session-ref> --server <server-ref>
tentgent session messages <session-ref> --tail 100
tentgent session update <session-ref> --title "Planning v2"
tentgent chat <model-ref> --session <session-ref> --message "user:Continue."
tentgent session rm <session-ref>
```

Read and mutate local sessions through the daemon:

```bash
curl -sS http://127.0.0.1:8790/v1/sessions \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS "http://127.0.0.1:8790/v1/sessions/<session-ref>/messages?tail=100" \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/sessions \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"title":"Planning","tags":["draft"]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/messages \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/compact \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"server_ref":"<server-ref>","keep_recent_messages":49}'
```

Session deletion is permanent. CLI chat uses stored context only with
`--session`; daemon chat requests are stateless. CLI session chat serializes
turns while the model response is running so transcript order stays stable. Sessions
are bounded working context: when they would exceed 50 messages, older messages
may be destructively summarized into one `system` summary message.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `session ls/inspect/messages/create/update/append/compact/rm` | `-H, --home <HOME>` | Optional Tentgent runtime home override for session state lookup |
| `session messages` | `--tail <N>` | Number of recent messages to show [default: 100] |
| `session create` | `--title <TITLE>` | Optional display title |
| `session create` | `--default-server <SERVER_REF>` | Optional default server reference stored as session metadata |
| `session create` | `--adapter <ADAPTER_REF>` | Optional adapter reference stored as session metadata |
| `session create` | `--tag <TAG>` | Tag to attach to the session. Can be repeated |
| `session update` | `--title <TITLE>` | Replace the display title |
| `session update` | `--clear-title` | Clear the display title |
| `session update` | `--default-server <SERVER_REF>` | Replace the default server reference |
| `session update` | `--clear-default-server` | Clear the default server reference |
| `session update` | `--adapter <ADAPTER_REF>` | Replace the adapter reference |
| `session update` | `--clear-adapter` | Clear the adapter reference |
| `session update` | `--tag <TAG>` | Replace the full tag list. Can be repeated |
| `session update` | `--clear-tags` | Clear all tags |
| `session append` | `--role <ROLE>` | Message role [possible values: system, user, assistant, tool] |
| `session append` | `--content <TEXT>` | Message content |
| `session append` | `--metadata-json <JSON>` | JSON object metadata to attach to the message [default: {}] |
| `session append` | `--compaction-server <SERVER_REF>` | Optional running server ref used to compact older session messages if the append would exceed the bounded session cap |
| `session compact` | `--server <SERVER_REF>` | Optional running server ref used for summary generation. Falls back to the session default server |
| `session compact` | `--keep-recent <N>` | Number of recent raw messages to keep next to the summary [default: 49] |
| `session compact` | `--instructions <TEXT>` | Additional summarization instructions |

## HTTP API

Start the [daemon](./daemon.md) and follow the [HTTP authentication and error rules](./api.md).

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/sessions` | None. |
| `POST` | `/v1/sessions` | `{"title":"optional","default_server_ref":"optional","adapter_ref":"optional","tags":[],"messages":[]}` |
| `GET` | `/v1/sessions/{reference}` | None. |
| `PATCH` | `/v1/sessions/{reference}` | `{"title":"new-or-null","default_server_ref":"new-or-null","adapter_ref":"new-or-null","tags":["..."]}` |
| `DELETE` | `/v1/sessions/{reference}` | None. |
| `GET` | `/v1/sessions/{reference}/messages?tail=100` | Session transcript tail. |
| `POST` | `/v1/sessions/{reference}/messages` | `{"messages":[{"role":"user","content":"...","server_ref":"optional","adapter_ref":"optional","metadata":{}}],"compaction_server_ref":"optional"}` |
| `POST` | `/v1/sessions/{reference}/compact` | `{"server_ref":"optional","keep_recent_messages":49,"instructions":"optional"}` |

Session messages are text records. Multimodal chat transcript content is not
implemented yet. Session endpoints manage stored records explicitly; daemon
chat does not automatically prepend or append them.

List returns `sessions`; inspect/update returns `session`; create also returns
`created`. Append returns `session` plus the `appended` messages. Message reads
return `messages`, `tail`, `total_messages`, `truncated`, and parsing warnings.
Compact returns `session` and `compacted` metadata. Use a running compatible
chat server when a compaction operation requires model execution.

## Related Guides

[Chat](./inference/chat.md) · [Model servers](./servers.md)
