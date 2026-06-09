# Provider API Error Semantics

This contract defines stable `400` error codes for provider-shaped OpenAI,
Claude/Anthropic, and Gemini API routes exposed by Tentgent.

It applies to:

- daemon provider compatibility routes such as `/v1/chat/completions`,
  `/v1/messages`, `/v1beta/models/{operation}`, `/v1/embeddings`, and
  `/v1/images/generations`
- direct cloud provider servers launched with `tentgent server run
  openai:<model>`, `anthropic:<model>`, `claude:<model>`, or `gemini:<model>`
- provider-shaped local ingress adapters launched with
  `tentgent server run <model-ref>`

It does not apply to native local model-bound server routes except when those
routes are implemented as provider-shaped ingress adapters.

## Error Shape

Provider compatibility errors use the daemon JSON error shape:

```json
{
  "error": "unsupported_provider_field",
  "message": "OpenAI-compatible tools and function calling require kernel tool-call support"
}
```

The `error` value is the stable machine-readable code. The `message` is
human-readable and may become more specific over time.

## Stable Codes

| Code | Meaning | Examples |
| --- | --- | --- |
| `unsupported_provider_field` | The upstream provider API has a known field, but Tentgent does not support that field in this compatibility route yet. | `tools`, `tool_choice`, non-text chat `response_format`, daemon/local OpenAI audio output `audio` / non-text `modalities`, direct cloud OpenAI audio output with `stream: true`, Claude-compatible `audio`, `modalities`, and `input_audio` fields, image `response_format`, image `n`, `stream_options.include_usage`, `web_search_options`, embedding `dimensions`, embedding `encoding_format: "base64"`, `stream=true` on direct cloud Claude messages. |
| `unsupported_provider_content` | A provider-shaped message, content part, or block uses a content type that this route cannot translate yet. | OpenAI daemon/local `image_url` and `input_audio` parts, malformed direct cloud OpenAI `image_url` or `input_audio` parts, Claude daemon/local `image`, `audio`, `input_audio`, `tool_use`, and `tool_result` blocks, direct cloud Claude audio blocks, URL/file image sources, or malformed base64 image blocks, Gemini daemon/local non-text parts, malformed direct cloud Gemini `inlineData` image/audio parts, Gemini `fileData`, and unknown direct cloud multimodal part shapes. |
| `unsupported_provider_operation` | A provider-shaped path operation is outside the supported endpoint family. | Gemini operations other than `generateContent` or `streamGenerateContent`. |
| `unsupported_provider_capability` | The selected provider, endpoint family, or bound local model capability cannot run the requested provider-compatible route. | Anthropic embeddings, Anthropic image generation, provider-shaped rerank requests on native `/v1/rerank`, OpenAI local embeddings requested from a chat-bound local server, OpenAI local image generation requested from a non-image-generation local server. |

All codes above return HTTP `400`.

## Unknown Fields

This contract stabilizes known unsupported provider fields first. It does not
make every provider-shaped request strict yet.

Current behavior:

- known unsupported fields listed above should reject with
  `unsupported_provider_field`
- local provider-shaped ingress routes check the server-bound capability before
  calling the Python runtime
- daemon embeddings manually reject unsupported top-level fields with
  `unsupported_provider_field`
- some request structs still ignore unknown fields because they do not use
  `#[serde(deny_unknown_fields)]`

Future compatibility work may make unknown-field handling stricter, but callers
should not rely on unknown provider fields being passed through.

## Runtime Errors

Provider/backend failures after request preflight are not provider compatibility
errors. They may still use runtime-specific error codes such as
`cloud_runtime_failed`, `embedding_runtime_failed`, or
`image_generation_runtime_failed`.

Kernel `UnsupportedTarget` errors from provider capability checks map to
`unsupported_provider_capability` for provider-shaped routes.
