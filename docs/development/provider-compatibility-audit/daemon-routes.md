# Daemon Provider-Compatible Routes

Daemon routes are mounted under the authenticated `/v1/*` router. These routes
are the most important user-facing compatibility surface for `v0.6.0`.

## Route Summary

| Route | Handler | Provider shape | Notes |
| --- | --- | --- | --- |
| `POST /v1/chat/completions` | `handlers/rest/chat/openai.rs` | OpenAI chat completions | Text chat plus OpenAI-shaped response and stream chunks. |
| `POST /v1/messages` | `handlers/rest/chat/claude.rs` | Claude/Anthropic messages | Text messages plus Claude-shaped response and stream events. |
| `POST /v1beta/models/{model}:generateContent` | `handlers/rest/chat/gemini.rs` | Gemini generateContent | Text-only Gemini-shaped response. |
| `POST /v1beta/models/{model}:streamGenerateContent` | `handlers/rest/chat/gemini.rs` | Gemini streamGenerateContent | Text-only Gemini-shaped SSE response. Query `alt=sse` is documented but not required by the operation parser. |
| `POST /v1/embeddings` | `handlers/rest/embedding/mod.rs` | Native plus OpenAI/Gemini-shaped request selection | Uses `model_ref` for local models and `model` or `provider` for cloud embeddings. |
| `POST /v1/images/generations` | `handlers/rest/images/cloud.rs` | OpenAI/Gemini image generation | Cloud-only image generation wrapper. Local image generation uses `/v1/images/generations/job`. |

## POST `/v1/chat/completions`

Handler:
`src/tentgent-daemon/src/handlers/rest/chat/openai.rs`

Required:

- `model`
- `messages`

Optional:

- `adapter_ref`
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
- `max_tokens`: no route-level default
- `temperature`: no route-level default
- `max_tokens` takes precedence over `max_completion_tokens`

Explicitly rejected:

- `tools`, deprecated `functions`, non-`none` `tool_choice`, non-`none`
  deprecated `function_call`, and `parallel_tool_calls: true`
- structured `response_format` values such as `json_object` and `json_schema`
- message `tool_calls`, deprecated message `function_call`, assistant `audio`,
  and assistant `refusal`
- `audio`
- `modalities` containing anything other than `text`
- non-text content parts such as `image_url`, `input_audio`, `file`, and
  `refusal`
- `n` values greater than `1`
- `stream_options.include_usage: true` and
  `stream_options.include_obfuscation: true`
- advanced generation controls that are not mapped to the kernel yet:
  `stop`, `top_p`, `frequency_penalty`, `presence_penalty`, `logit_bias`,
  `logprobs`, `top_logprobs`, `prediction`, `reasoning_effort`, and
  `verbosity`
- provider-side metadata, storage, cache, safety, and service fields:
  `metadata`, `store: true`, `seed`, `service_tier`, `user`,
  `safety_identifier`, `prompt_cache_key`, and `prompt_cache_retention`
- `web_search_options`
- roles outside `developer`, `system`, `user`, and `assistant`

Currently ignored:

- Unknown top-level fields.
- Unknown fields inside known nested structs, unless later validation reads
  them.
- Message `name` fields.

Example request:

```json
{
  "model": "gpt-4.1-mini",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 64,
  "temperature": 0.0,
  "stream": false
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
      "finish_reason": "stop",
      "logprobs": null
    }
  ],
  "usage": null
}
```

Model-specific notes:

- No route-level model profile exists yet.
- Provider or local runtime may still impose context, token, and temperature
  limits.
- OpenAI image parts are rejected on the daemon route; use a direct cloud
  provider server for current image-part pass-through.

## POST `/v1/messages`

Handler:
`src/tentgent-daemon/src/handlers/rest/chat/claude.rs`

Required:

- `model`
- `messages`

Optional:

- `system`
- `adapter_ref`
- `max_tokens`
- `temperature`
- `stream`

Defaults:

- `stream`: `false`
- `max_tokens`: no daemon route-level default
- `temperature`: no route-level default

Explicitly rejected:

- `tools`, `tool_choice`
- non-text content blocks such as `image`
- roles outside `system`, `user`, and `assistant`

Currently ignored:

- Unknown top-level fields.
- Unknown fields inside known nested structs, unless later validation reads
  them.

Example request:

```json
{
  "model": "claude-3-5-sonnet-latest",
  "system": "Answer briefly.",
  "messages": [{"role": "user", "content": "Hello"}],
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
  "stop_sequence": null,
  "usage": null
}
```

Model-specific notes:

- No route-level model profile exists yet.
- Claude image blocks are rejected on the daemon route.
- Direct cloud server `/v1/messages` accepts base64 image blocks, but it is
  non-streaming today.

## POST `/v1beta/models/{model}:generateContent`

Handler:
`src/tentgent-daemon/src/handlers/rest/chat/gemini.rs`

Required:

- path `{model}`
- `contents`

Optional:

- `adapter_ref`
- `generationConfig.maxOutputTokens`
- `generationConfig.temperature`
- `systemInstruction`

Defaults:

- no route-level default for `maxOutputTokens`
- no route-level default for `temperature`

Explicitly rejected:

- `tools`, `toolConfig`
- non-text parts
- unsupported operation suffixes
- empty model path
- roles outside `user`, `model`, `assistant`, and `system`

Currently ignored:

- Unknown top-level fields.
- Unknown `generationConfig` fields.

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

- No route-level model profile exists yet.
- Only text parts are accepted in daemon Gemini compatibility routes.
- The route parser recognizes `:streamGenerateContent` regardless of whether
  `alt=sse` is present.

## POST `/v1/embeddings`

Handler:
`src/tentgent-daemon/src/handlers/rest/embedding/mod.rs`

Required:

- `input`
- one of `model_ref` or `model`

Optional:

- `provider`
- `encoding_format`, only when set to `float`

Defaults:

- `provider`: OpenAI when `model` is present and `provider` is omitted

Explicitly rejected:

- Unknown fields.
- Missing `model_ref` or `model`.
- Non-string `model_ref` or `model`.
- Missing `input`.
- `input` values that are not a string or string array.
- Empty input arrays or empty strings.
- `dimensions`
- `encoding_format: "base64"` or non-`float` values.
- `user`
- Provider names outside OpenAI, Gemini/Google, Anthropic/Claude.

Currently ignored:

- No known ignored top-level fields.

Example cloud request:

```json
{
  "model": "text-embedding-3-small",
  "input": ["first text", "second text"]
}
```

Example Gemini cloud request:

```json
{
  "provider": "gemini",
  "model_ref": "text-embedding-004",
  "input": "one text"
}
```

Example OpenAI-compatible response:

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

Model-specific notes:

- OpenAI-compatible requests using `model` and provider OpenAI return an
  OpenAI-style embedding list.
- Native local embeddings and non-OpenAI cloud embedding requests remain
  Tentgent-shaped.
- Local embedding models still use `model_ref` and require stored `embedding`
  capability metadata.
- `dimensions` is not accepted by this handler.

## POST `/v1/images/generations`

Handler:
`src/tentgent-daemon/src/handlers/rest/images/cloud.rs`

Required:

- `model`
- `prompt`

Optional:

- `size`
- `provider`

Defaults:

- `provider`: OpenAI
- `n`: fixed internally to `1` for provider requests

Explicitly rejected:

- Unsupported image provider strings during provider deserialization.
- Unsupported provider/capability combinations such as Anthropic image
  generation.
- `response_format`
- `n`

Currently ignored:

- Unknown top-level fields other than known unsupported provider fields.

Example request:

```json
{
  "provider": "openai",
  "model": "gpt-image-1",
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

- The daemon provider defaults to OpenAI when `provider` is omitted.
- `gpt-image-*` requests intentionally omit the legacy OpenAI
  `response_format` field.
- Non-`gpt-image-*` OpenAI image requests send `response_format = "b64_json"`.
- Gemini maps `size` to `sampleImageSize`; provider/model support may vary.
