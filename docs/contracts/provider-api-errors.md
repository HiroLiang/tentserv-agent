# Provider API Error Semantics

This contract defines stable `400` error codes for provider-shaped OpenAI,
Claude/Anthropic, and Gemini API routes exposed by Tentgent.

It applies to:

- daemon provider compatibility routes such as `/v1/chat/completions`,
  `/v1/messages`, `/v1beta/models/{operation}`, `/v1/embeddings`, and
  `/v1/images/generations`
- direct cloud provider servers launched with `tentgent server run
  openai:<model>`, `anthropic:<model>`, `claude:<model>`, or `gemini:<model>`

It does not apply to native local model-bound server routes except when those
routes are listed as fallback context in user-facing compatibility docs.

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
| `unsupported_provider_field` | The upstream provider API has a known field, but Tentgent does not support that field in this compatibility route yet. | `tools`, `tool_choice`, `functions`, `function_call`, `response_format`, `dimensions`, `stream=true` on direct cloud Claude messages. |
| `unsupported_provider_content` | A provider-shaped message, content part, or block uses a content type that this route cannot translate yet. | OpenAI daemon `image_url` parts, Claude daemon `image` blocks, Gemini daemon non-text parts, unknown direct cloud multimodal part shapes. |
| `unsupported_provider_operation` | A provider-shaped path operation is outside the supported endpoint family. | Gemini operations other than `generateContent` or `streamGenerateContent`. |
| `unsupported_provider_capability` | The selected provider or endpoint family is not implemented through Tentgent yet. | Anthropic embeddings, Anthropic image generation. |

All codes above return HTTP `400`.

## Unknown Fields

This contract stabilizes known unsupported provider fields first. It does not
make every provider-shaped request strict yet.

Current behavior:

- known unsupported fields listed above should reject with
  `unsupported_provider_field`
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
