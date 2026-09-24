# Text Chat

Run one request against a managed chat model. Prepare the [runtime](../runtime.md), [pull a model](../models.md), and inspect its support before inference.

## Examples And Common Operations

Run one-shot chat:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

Run one-shot chat with an adapter:

```bash
tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Give one practical suggestion for organizing my desk." \
  --max-tokens 128
```

Stream a response, or keep a bounded local conversation:

```bash
tentgent chat <model-ref> --message "user:Hello" --max-tokens 128 --temperature 0 --stream
tentgent session create --title "My conversation"
tentgent chat <model-ref> --session <session-ref> --message "user:Remember the topic: gardening."
tentgent chat <model-ref> --session <session-ref> --message "user:Suggest a next step."
```

`--message` is repeatable and accepts `system:`, `user:`, or `assistant:` prefixes. The default role is user. With no message, the CLI prompts once. Session behavior is explained in [Sessions](../sessions.md).

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `chat` | `-m, --message <MESSAGE>` | Message content in order. Use role:content for explicit system, user, or assistant context |
| `chat` | `-H, --home <HOME>` | Optional Tentgent runtime home override passed through to the Python harness |
| `chat` | `-n, --max-tokens <N>` | Maximum number of tokens to generate |
| `chat` | `-T, --temperature <TEMP>` | Sampling temperature. Omit or use 0 for deterministic decoding |
| `chat` | `-a, --adapter-ref <REF>` | Compatible managed MLX or PEFT chat adapter reference. |
| `chat` | `--session <SESSION_REF>` | Optional local session ref to use for context and transcript recording |
| `chat` | `--max-session-messages <N>` | Maximum number of prior session messages to prepend when --session is used |
| `chat` | `-s, --stream` | Stream generated text to stdout when the selected backend supports streaming |

## HTTP API

Start the [daemon](../daemon.md) and follow the [HTTP authentication and error rules](../api.md).

Native Tentgent chat:

```http
POST /v1/chat
Content-Type: application/json
```

```json
{
  "model_ref": "<chat-model-ref>",
  "adapter_ref": "<optional-adapter-ref>",
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 128,
  "temperature": 0.0,
  "stream": false
}
```

`messages[].role` supports `system`, `user`, and `assistant`. `stream=true`
returns Server-Sent Events.

Native daemon chat requires `model_ref` and `messages`; it executes the model
through the kernel. `server_ref`, `session_ref`, and `max_session_messages` are
not accepted. Use CLI `chat --session` for automatic transcript context, or
manage context explicitly through the [session API](../sessions.md#http-api).
Non-streaming response:

```json
{
  "text": "Hello!",
  "finish_reason": "stop",
  "model_ref": "<resolved-model-ref>",
  "adapter_ref": null
}
```

`adapter_ref` is null when no adapter is selected. For native SSE events, see the
[chat contract](../../contracts/http-daemon.md#native-chat).

Compatibility adapters route to the same chat execution path and are text-only:

| Method | Path | Request notes |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI-style `model`, `messages`, optional `adapter_ref`, `max_tokens`, `max_completion_tokens`, `temperature`, `stream`. |
| `POST` | `/v1/messages` | Claude-style `model`, `messages`, optional `system`, `adapter_ref`, `max_tokens`, `temperature`, `stream`. |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini-style `contents`, optional `systemInstruction`, `generationConfig`, `adapter_ref`. |
| `POST` | `/v1beta/models/{model}:streamGenerateContent?alt=sse` | Gemini-style streaming response. |

The daemon registers these Gemini operations through
`POST /v1beta/models/{*operation}`. Unknown operation suffixes are rejected;
the two concrete paths above are the callable public shapes.

Tools, function calling, audio content, and non-text message parts are rejected
by chat compatibility routes until their corresponding adapters exist. Send
single-image local vision requests through [native Vision Chat](./vision.md#http-api).

For provider-shaped route coverage across OpenAI, Claude/Anthropic, and Gemini
APIs, see [provider-compatibility.md](../provider-compatibility.md).
For copy-paste provider-compatible curl and SDK examples, see
[provider-compatible-examples.md](../provider-compatible-examples.md).

### Curl, Streaming, And Adapter Selection

```bash
curl -sS http://127.0.0.1:8790/v1/chat \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","messages":[{"role":"user","content":"Hello"}],"max_tokens":128,"temperature":0}'
tentgent server run <model-ref> --capability chat --port 8780 --detach
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"adapter_ref":"<adapter-ref>","max_tokens":128,"temperature":0,"stream":true}'
```

Omit `adapter_ref` for the base model. On the direct server port, omit
`model_ref` as well: the server uses its bound model. Both native endpoints
are stateless. Direct-server native SSE uses `delta`, `done`, and `error`
events; clients should use the
[streaming contract](../../contracts/server-chat.md) rather than assuming the
OpenAI `choices` envelope. The daemon has its own
[native SSE contract](../../contracts/http-daemon.md#native-chat).

Actual output quality and streaming availability depend on the model/backend.
Use a compatible chat adapter; capability metadata alone does not prove that
its weights can execute on the selected backend.

## Related Guides

[Adapters](../adapters.md) · [Sessions](../sessions.md) · [Persistent servers](../servers.md) · [Provider-compatible examples](../provider-compatible-examples.md)
