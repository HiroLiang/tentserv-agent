# Direct Cloud Provider Server Routes

Direct cloud provider servers are launched through `tentgent server run
openai:<model>`, `anthropic:<model>`, `claude:<model>`, or `gemini:<model>`.
The process requires the provider API key in the provider environment variable
at launch time and binds all routes to one provider and one provider model.

This document does not cover `tentgent server run <local-model-ref>`. Local
model-bound servers are native Tentgent servers and are covered in
[local-model-boundary.md](./local-model-boundary.md).

## Route Summary

| Route | Handler | Provider shape | Notes |
| --- | --- | --- | --- |
| `POST /v1/chat` | `cloud_server.rs` | Tentgent native chat shape over a cloud provider | Bound model; returns native `text` response. |
| `POST /v1/chat/completions` | `cloud_server.rs` | OpenAI chat completions | Supports text and `image_url` content parts. |
| `POST /v1/messages` | `cloud_server.rs` | Claude/Anthropic messages | Supports text blocks and base64 image blocks; non-streaming only. |
| `POST /v1beta/models/{operation}` | `cloud_server.rs` | Gemini generateContent/streamGenerateContent | Supports text and inline image data. |
| `POST /v1/embeddings` | `cloud_server.rs` | Embedding request shape over bound provider model | OpenAI-bound servers return OpenAI-style embedding lists; other providers currently return Tentgent embedding shape. |
| `POST /v1/images/generations` | `cloud_server.rs` | Image generation over bound provider model | Request omits `model`; server uses bound provider model. |

## POST `/v1/chat`

Required:

- `messages`

Optional:

- `max_tokens`
- `temperature`
- `stream`

Defaults:

- `stream`: `false`
- bound provider model from server launch

Explicitly rejected:

- No route-specific unsupported-field checks beyond JSON shape errors.

Currently ignored:

- Unknown top-level fields.
- Provider-specific fields not represented in `NativeChatRequest`.

Example request:

```json
{
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 64,
  "temperature": 0.0
}
```

Example response:

```json
{
  "text": "...",
  "finish_reason": "stop",
  "model_ref": "gpt-4.1-mini",
  "adapter_ref": null
}
```

Model-specific notes:

- This is a native Tentgent wrapper over a bound cloud model, not a
  provider-compatible route.
- No route-level model profile exists yet.

## POST `/v1/chat/completions`

Required:

- `messages`

Optional:

- `max_tokens`
- `max_completion_tokens`
- `n`, only when set to `1`
- `temperature`
- `stream`
- `stream_options`, only when `include_usage` and `include_obfuscation` are
  unset or `false`
- `modalities`, only when every value is `text`
- `response_format`, only when set to `{ "type": "text" }`
- `tool_choice` and deprecated `function_call`, only when set to `none`
- `parallel_tool_calls`, only when set to `false`
- `store`, only when set to `false`

Defaults:

- `stream`: `false`
- bound provider model from server launch
- caller-supplied `model` is ignored if sent

Accepted content:

- text content
- OpenAI `image_url` parts

Explicitly rejected:

- `tools`, deprecated `functions`, non-`none` `tool_choice`, non-`none`
  deprecated `function_call`, and `parallel_tool_calls: true`
- structured `response_format` values such as `json_object` and `json_schema`
- `audio`
- `modalities` containing anything other than `text`
- unsupported OpenAI content part types such as `input_audio`, `file`, and
  `refusal`
- missing `image_url` payload for an `image_url` content part
- message `tool_calls`, deprecated message `function_call`, assistant `audio`,
  and assistant `refusal`
- `n` values greater than `1`
- `stream_options.include_usage: true` and
  `stream_options.include_obfuscation: true`
- advanced generation controls that are not mapped to cloud chat requests yet:
  `stop`, `top_p`, `frequency_penalty`, `presence_penalty`, `logit_bias`,
  `logprobs`, `top_logprobs`, `prediction`, `reasoning_effort`, and
  `verbosity`
- provider-side metadata, storage, cache, safety, and service fields:
  `metadata`, `store: true`, `seed`, `service_tier`, `user`,
  `safety_identifier`, `prompt_cache_key`, and `prompt_cache_retention`
- `web_search_options`

Currently ignored:

- Unknown top-level fields.
- Caller-supplied `model`.
- Message `name` fields.

Example request:

```json
{
  "messages": [
    {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
  ],
  "max_tokens": 64,
  "temperature": 0.0
}
```

Example response:

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1760000000,
  "model": "gpt-4.1-mini",
  "choices": [
    {
      "index": 0,
      "message": {"role": "assistant", "content": "..."},
      "finish_reason": "stop"
    }
  ],
  "usage": null
}
```

Model-specific notes:

- Streaming returns generic Tentgent `delta` and `done` SSE events, not OpenAI
  `chat.completion.chunk` events.
- The bound provider model may support image parts even though daemon
  `/v1/chat/completions` rejects them.

## POST `/v1/messages`

Required:

- `messages`

Optional:

- `system`
- `max_tokens`
- `temperature`

Defaults:

- bound provider model from server launch
- non-streaming only in this direct cloud handler

Accepted content:

- text content
- Claude text blocks
- Claude image blocks with base64 source

Explicitly rejected:

- unsupported Claude content block types
- missing image source
- non-base64 image sources

Currently ignored:

- Unknown top-level fields.
- `stream`, `model`, `tools`, and `tool_choice`.

Example request:

```json
{
  "system": "Answer briefly.",
  "messages": [
    {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
  ],
  "max_tokens": 64,
  "temperature": 0.0
}
```

Example response:

```json
{
  "id": "msg-...",
  "type": "message",
  "role": "assistant",
  "content": [{"type": "text", "text": "..."}],
  "model": "claude-3-5-sonnet-latest",
  "stop_reason": "end_turn",
  "usage": null
}
```

Model-specific notes:

- Direct cloud Claude messages do not expose a streaming response path today,
  even if `stream` is sent.
- `system` is a string here, unlike the daemon route where `system` uses the
  broader `ClaudeContent` parser.

## POST `/v1beta/models/{operation}`

Required:

- `contents`

Optional:

- `systemInstruction`
- `generationConfig.maxOutputTokens`
- `generationConfig.temperature`

Defaults:

- bound provider model from server launch
- streaming is selected by an operation ending in `:streamGenerateContent`

Accepted content:

- text parts
- Gemini `inlineData` image parts

Explicitly rejected:

- unsupported Gemini part shapes

Currently ignored:

- Unknown top-level fields.
- `tools` and `toolConfig`.
- Path model, because the bound provider model is used instead.

Example request:

```json
{
  "contents": [
    {"role": "user", "parts": [{"text": "Hello"}]}
  ],
  "generationConfig": {
    "maxOutputTokens": 64,
    "temperature": 0.0
  }
}
```

Example response:

```json
{
  "candidates": [
    {
      "index": 0,
      "content": {"role": "model", "parts": [{"text": "..."}]},
      "finishReason": "STOP"
    }
  ],
  "usageMetadata": null,
  "modelVersion": "gemini-2.0-flash"
}
```

Model-specific notes:

- Streaming returns generic Tentgent `delta` and `done` SSE events, not the
  daemon Gemini-shaped stream mapper.
- Any operation not ending in `:streamGenerateContent` is treated as
  non-streaming.

## POST `/v1/embeddings`

Required:

- `input`

Optional:

- `encoding_format`, only when set to `float`

Defaults:

- bound provider model from server launch

Explicitly rejected:

- empty input arrays or empty strings
- `dimensions`
- `encoding_format: "base64"` or non-`float` values
- `user`

Currently ignored:

- Unknown top-level fields.
- `model` and `provider`, because the bound provider model is used instead.

Example request:

```json
{
  "input": ["first text", "second text"]
}
```

Example OpenAI-bound response:

```json
{
  "object": "list",
  "data": [
    {"object": "embedding", "index": 0, "embedding": [0.1, 0.2]},
    {"object": "embedding", "index": 1, "embedding": [0.3, 0.4]}
  ],
  "model": "text-embedding-3-small",
  "usage": null
}
```

Example non-OpenAI response:

```json
{
  "model_ref": "text-embedding-3-small",
  "data": [
    {"index": 0, "embedding": [0.1, 0.2]},
    {"index": 1, "embedding": [0.3, 0.4]}
  ]
}
```

Model-specific notes:

- OpenAI-bound cloud servers return OpenAI-style embedding responses.
- Gemini-bound cloud servers currently return Tentgent-shaped embedding
  responses.
- `dimensions` is rejected and not forwarded.

## POST `/v1/images/generations`

Required:

- `prompt`

Optional:

- `size`

Defaults:

- bound provider model from server launch
- `n`: fixed internally to `1` by the cloud client

Explicitly rejected:

- Missing required JSON fields through deserialization.

Currently ignored:

- Unknown top-level fields.
- `model`, `provider`, `n`, and `response_format`.

Example request:

```json
{
  "prompt": "A small watercolor house",
  "size": "1024x1024"
}
```

Example response:

```json
{
  "created": 1760000000,
  "data": [{"b64_json": "..."}]
}
```

Model-specific notes:

- The server always uses the bound provider model.
- OpenAI and Gemini provider mapping is handled by the cloud client.
- Anthropic image generation should be rejected when the server spec is
  prepared; if reached at request time, the cloud client reports unsupported.
