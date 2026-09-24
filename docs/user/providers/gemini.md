# Gemini-Compatible APIs

Choose a [serving surface and base URL](./README.md#base-urls) first. Check the [compatibility matrix](../provider-compatibility.md) for supported fields and capabilities. Examples use a loopback daemon without a bearer token; add the [authentication header](../api.md) when configured.

## Curl Examples

### Generate Content

Works on daemon, local chat model-bound servers, and direct Gemini cloud
provider servers:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS "$TENTGENT_BASE_URL/v1beta/models/gemini-2.5-flash:generateContent" \
  -H 'Content-Type: application/json' \
  -d '{
    "systemInstruction": {
      "parts": [{"text": "Answer briefly."}]
    },
    "contents": [
      {"role": "user", "parts": [{"text": "Say hello in Traditional Chinese."}]}
    ],
    "generationConfig": {
      "maxOutputTokens": 64,
      "temperature": 0.0
    }
  }'
```

On direct cloud Gemini servers, the path model is accepted but ignored because
the server is bound to the provider model from launch. Direct cloud Gemini can
translate text, `inlineData` image parts, and `inlineData` audio parts for
compatible models. Daemon and local model-bound Gemini routes are text-only
today.

Direct cloud Gemini image understanding uses `inlineData`:

```bash
tentgent server run gemini:gemini-2.5-flash \
  --host 127.0.0.1 \
  --port 8793
```

```bash
curl -sS http://127.0.0.1:8793/v1beta/models/ignored:generateContent \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [{
      "role": "user",
      "parts": [
        {"text": "Caption this image."},
        {
          "inlineData": {
            "mimeType": "image/png",
            "data": "AA=="
          }
        }
      ]
    }]
  }'
```

Direct cloud Gemini audio understanding also uses `inlineData`:

```bash
tentgent server run gemini:gemini-2.5-flash \
  --host 127.0.0.1 \
  --port 8793

AUDIO_B64=$(base64 < test-data/we_go_up.mp3 | tr -d '\n')

curl -sS http://127.0.0.1:8793/v1beta/models/ignored:generateContent \
  -H 'Content-Type: application/json' \
  -d "{
    \"contents\": [{
      \"role\": \"user\",
      \"parts\": [
        {\"text\": \"Summarize this audio in one sentence.\"},
        {
          \"inlineData\": {
            \"mimeType\": \"audio/mp3\",
            \"data\": \"$AUDIO_B64\"
          }
        }
      ]
    }]
  }"
```

Use a `generateContent`-capable Gemini model such as `gemini-2.5-flash` for
this route. Native Audio Dialog models such as
`gemini-2.5-flash-preview-native-audio-dialog` and
`gemini-2.5-flash-native-audio-preview-12-2025` are Live API models and are not
supported by `v1beta generateContent`.

Daemon and local model-bound Gemini routes reject inline image and audio parts
until the local multimodal context pipeline exists:

```bash
curl -sS "$TENTGENT_BASE_URL/v1beta/models/gemini-2.5-flash:generateContent" \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [{
      "role": "user",
      "parts": [
        {"text": "Summarize this audio."},
        {
          "inlineData": {
            "mimeType": "audio/mp3",
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

### Stream Generate Content

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS -N \
  "$TENTGENT_BASE_URL/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse" \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [
      {"role": "user", "parts": [{"text": "Say hello in Traditional Chinese."}]}
    ],
    "generationConfig": {
      "maxOutputTokens": 64,
      "temperature": 0.0
    }
  }'
```

Streaming returns Gemini-shaped SSE `data:` frames. It does not introduce
OpenAI or Claude event names.

### Gemini Embeddings

Daemon Gemini cloud embeddings use the existing Tentgent `/v1/embeddings`
route, not the official Gemini `embedContent` path:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS "$TENTGENT_BASE_URL/v1/embeddings" \
  -H 'Content-Type: application/json' \
  -d '{
    "provider": "gemini",
    "model": "gemini-embedding-001",
    "input": "one text"
  }'
```

Direct Gemini cloud embedding servers also expose `/v1/embeddings`, but their
response shape is currently native Tentgent-shaped rather than OpenAI-shaped.
Official Gemini `embedContent`, rerank, Files API media parts, and broader
multimodal local Gemini ingress remain future compatibility work.

## Unsupported Behavior

These requests intentionally exercise rejected fields or capabilities. Error blocks show the stable code; responses also carry a human-readable message.

Gemini tools are not supported by compatibility adapters today:

```bash
curl -sS "$TENTGENT_BASE_URL/v1beta/models/gemini-2.5-flash:generateContent" \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [
      {"role": "user", "parts": [{"text": "Call a tool."}]}
    ],
    "tools": [{"functionDeclarations": []}]
  }'
```

Expected stable error code:

```json
{
  "error": "unsupported_provider_field"
}
```
