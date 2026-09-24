# Vision Chat

Prepare the [local-model runtime profile](../runtime.md#media-runtime-dependencies), a model with the matching capability, and any input files. See [file and HTTP rules](./README.md#file-and-http-media-rules) before choosing CLI or daemon execution.

## Examples And Common Operations

### Vision Chat

Run foreground vision chat without starting the daemon:

```bash
tentgent vision chat /absolute/path/image.png \
  --model-ref <vision-chat-model-ref> \
  --prompt "Describe this image in one sentence." \
  --output answer.md \
  --format md
```

With `--output`, the command writes only to the requested file and prints a
short completion message. It fails if the output file already exists. Without
`--output`, `text` and `md` print the generated answer to stdout; `json` prints
the response envelope.

Pull a small model before running local vision chat:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull HuggingFaceTB/SmolVLM-256M-Instruct --capability vision-chat
```

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `vision chat` | `-m, --model-ref <MODEL_REF>` | Stored Tentgent vision-chat model reference to run |
| `vision chat` | `-p, --prompt <TEXT>` | Prompt to ask about the image |
| `vision chat` | `--system-prompt <TEXT>` | Optional system prompt |
| `vision chat` | `-o, --output <OUTPUT_PATH>` | Local output path. Existing files are never overwritten |
| `vision chat` | `--format <FORMAT>` | Output format intent: text, json, or md [default: text] |
| `vision chat` | `--max-tokens <N>` | Optional max generated tokens |
| `vision chat` | `--temperature <FLOAT>` | Optional sampling temperature |
| `vision chat` | `-H, --home <HOME>` | Optional Tentgent runtime home override |

## HTTP API

Start the [daemon](../daemon.md) and follow the [HTTP authentication and error rules](../api.md).

### Vision Chat

Native vision chat accepts one image plus one text prompt and returns generated
text in a JSON envelope. It is a bounded synchronous request, not a durable job.

```http
POST /v1/vision/chat
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | Exactly one image. The daemon does not receive or trust the client's local path. |
| `model_ref` | yes | text | Local `vision-chat` model ref or unique alias. |
| `prompt` | yes | text | User prompt for the image. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `json`, or `md`; defaults to `text`. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional sampling temperature. |

Accepted image media types are `image/png`, `image/jpeg`, and `image/webp`.
The daemon writes uploaded bytes to a request-scoped temp file and removes it
after success or failure; the selected runtime sees a complete image file. The
daemon-wide media upload cap applies to the `image` file part.

```bash
curl -sS http://127.0.0.1:8790/v1/vision/chat \
  -F model_ref=<vision-chat-model-ref> \
  -F prompt='Describe this image in one sentence.' \
  -F output_format=text \
  -F image=@/absolute/path/image.png
```

Response:

```json
{
  "model_ref": "<vision-chat-model-ref>",
  "output_format": "text",
  "text": "A generated answer about the image.",
  "finish_reason": "stop"
}
```

OpenAI, Claude, and Gemini compatible multimodal payloads are not accepted yet.
Those adapters should map into this native vision contract in a later slice.

## Related Guides

[Model fixtures](../model-fixtures.md) · [Jobs and results](../jobs.md) · [Inference index](./README.md)
