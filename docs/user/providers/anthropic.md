# Anthropic / Claude-Compatible APIs

Choose a [serving surface and base URL](./README.md#base-urls) first. Check the [compatibility matrix](../provider-compatibility.md) for supported fields and capabilities. Examples use a loopback daemon without a bearer token; add the [authentication header](../api.md) when configured.

## Curl Examples

### Messages

Works on daemon, local chat model-bound servers, and direct Claude cloud
provider servers:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS "$TENTGENT_BASE_URL/v1/messages" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "claude-3-5-sonnet-latest",
    "system": "Answer briefly.",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": false
  }'
```

Daemon and local model-bound Claude routes support text-only streaming with
`stream: true`. Direct cloud Claude `/v1/messages` is non-streaming today and
rejects `stream: true`.

Direct cloud Claude servers accept base64 image blocks for compatible models.
The supported media types are `image/jpeg`, `image/png`, `image/gif`, and
`image/webp`.

```bash
tentgent server run claude:claude-sonnet-4-5 \
  --host 127.0.0.1 \
  --port 8792
```

```bash
curl -sS http://127.0.0.1:8792/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "max_tokens": 128,
    "messages": [{
      "role": "user",
      "content": [
        {
          "type": "image",
          "source": {
            "type": "base64",
            "media_type": "image/png",
            "data": "AA=="
          }
        },
        {"type": "text", "text": "Describe this image."}
      ]
    }]
  }'
```

Claude URL image sources and Files API image sources are not implemented in
Tentgent direct cloud compatibility yet. Daemon and local model-bound Claude
routes reject image blocks, tool use, and tool results until local tool-call
and multimodal adapters are implemented.

Claude-compatible audio input and output are not implemented on daemon, local
model-bound, or direct cloud Claude routes. Audio-shaped message blocks and
fields fail before local runtime or Anthropic upstream dispatch:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/messages" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 64,
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "Transcribe this."},
        {
          "type": "audio",
          "source": {
            "type": "base64",
            "media_type": "audio/wav",
            "data": "AA=="
          }
        }
      ]
    }]
  }'
```

Expected stable error code:

```json
{
  "error": "unsupported_provider_content"
}
```

Claude-compatible audio output fields are also unsupported:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/messages" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 64,
    "messages": [{"role": "user", "content": "Say hello."}],
    "modalities": ["text", "audio"],
    "audio": {"voice": "alloy", "format": "wav"}
  }'
```

Expected stable error code:

```json
{
  "error": "unsupported_provider_field"
}
```

## SDK Examples

SDKs usually require a non-empty API key even when the local Tentgent daemon or
server does not enforce authentication. Use a placeholder key for loopback
servers without `TENTGENT_DAEMON_TOKEN`. When daemon bearer auth is enabled,
configure the SDK so the request sends
`Authorization: Bearer $TENTGENT_DAEMON_TOKEN`.

### Anthropic Python

```python
from anthropic import Anthropic

client = Anthropic(
    base_url="http://127.0.0.1:8790",
    auth_token="tentgent-local",
)

message = client.messages.create(
    model="claude-3-5-sonnet-latest",
    max_tokens=64,
    messages=[{"role": "user", "content": "Say hello in Traditional Chinese."}],
)

print(message.content[0].text)
```

Gemini examples are REST-only for now. Do not assume the official Gemini SDK can
be pointed at Tentgent by changing one base URL until that SDK flow is tested
and documented.

## Unsupported Behavior

These requests intentionally exercise rejected fields or capabilities. Error blocks show the stable code; responses also carry a human-readable message.

Anthropic embeddings are not implemented.

```bash
curl -sS "$TENTGENT_BASE_URL/v1/embeddings" \
  -H 'Content-Type: application/json' \
  -d '{
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-latest",
    "input": "one text"
  }'
```

Expected stable error code:

```json
{
  "error": "unsupported_provider_capability"
}
```
