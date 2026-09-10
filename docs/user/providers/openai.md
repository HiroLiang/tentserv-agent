# OpenAI-Compatible APIs

Choose a [serving surface and base URL](./README.md#base-urls) first. Check the [compatibility matrix](../provider-compatibility.md) for supported fields and capabilities. Examples use a loopback daemon without a bearer token; add the [authentication header](../api.md) when configured.

## Curl Examples

### Chat Completions

Works on daemon, local chat model-bound servers, and direct cloud provider
servers:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS "$TENTGENT_BASE_URL/v1/chat/completions" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "<chat-model-ref-or-provider-model>",
    "messages": [
      {"role": "system", "content": "Answer briefly."},
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": false
  }'
```

Use `stream: true` for Server-Sent Events on daemon and local model-bound
OpenAI chat ingress. Direct cloud provider servers also accept OpenAI
`image_url` content parts for compatible cloud models. Daemon and local
model-bound OpenAI chat routes are text-only today.

Direct OpenAI cloud vision input:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8783

curl -sS "$TENTGENT_BASE_URL/v1/chat/completions" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "ignored-by-direct-cloud-server",
    "messages": [
      {
        "role": "user",
        "content": [
          {"type": "text", "text": "Describe this image."},
          {
            "type": "image_url",
            "image_url": {
              "url": "https://example.com/cat.png",
              "detail": "low"
            }
          }
        ]
      }
    ],
    "max_tokens": 64
  }'
```

Use that shape only with `tentgent server run openai:<vision-model>`. Daemon
and local model-bound OpenAI chat routes reject `image_url` until local
multimodal routing is implemented.

### Embeddings

Daemon OpenAI cloud embeddings:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS "$TENTGENT_BASE_URL/v1/embeddings" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "text-embedding-3-small",
    "input": ["first text", "second text"],
    "encoding_format": "float"
  }'
```

Local embedding model-bound servers and direct OpenAI cloud embedding servers
also accept the same OpenAI-shaped body at `/v1/embeddings`. In those server
modes, the bound model from `tentgent server run ...` is used and the caller
`model` value is ignored.

### Image Generation

Daemon OpenAI image generation:

```bash
export TENTGENT_BASE_URL=http://127.0.0.1:8790

curl -sS "$TENTGENT_BASE_URL/v1/images/generations" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-image-1",
    "prompt": "A small watercolor house",
    "size": "1024x1024"
  }'
```

Daemon Gemini image generation uses the same OpenAI-shaped Tentgent route with
an explicit `provider` selector:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/images/generations" \
  -H 'Content-Type: application/json' \
  -d '{
    "provider": "gemini",
    "model": "gemini-2.5-flash-image",
    "prompt": "A small watercolor house",
    "size": "1024x1024"
  }'
```

Direct cloud image-generation servers and local image-generation model-bound
servers also expose `/v1/images/generations`. In direct cloud mode, send
`prompt` and optional `size`; the bound provider model from
`tentgent server run <provider>:<model>` is used. In local model-bound mode,
the bound local image-generation model is used and caller provider selection is
rejected.

Direct cloud Gemini image-generation servers should be launched with a Gemini
image model such as `gemini-2.5-flash-image`, `gemini-3.1-flash-image`, or
`gemini-3-pro-image`:

```bash
tentgent server run gemini:gemini-2.5-flash-image \
  --host 127.0.0.1 \
  --port 8793

curl -sS http://127.0.0.1:8793/v1/images/generations \
  -H 'Content-Type: application/json' \
  -d '{
    "prompt": "A small watercolor house",
    "size": "1024x1024"
  }'
```

Provider-compatible image generation is intentionally narrower than OpenAI:
`n` and `response_format` are rejected, and responses are always
OpenAI-shaped `b64_json` envelopes. For Gemini image models, Tentgent maps the
request to Gemini `generateContent` and extracts the returned `inlineData`
image. Imagen model names keep using Gemini `predict`. Other Gemini model names
are rejected as unsupported image-generation targets before upstream dispatch.
The OpenAI-shaped `size` field is accepted for route parity on Gemini image
models but is not forwarded today; Imagen model fallback forwards `size` as
`sampleImageSize`.

## SDK Examples

SDKs usually require a non-empty API key even when the local Tentgent daemon or
server does not enforce authentication. Use a placeholder key for loopback
servers without `TENTGENT_DAEMON_TOKEN`. When daemon bearer auth is enabled,
configure the SDK so the request sends
`Authorization: Bearer $TENTGENT_DAEMON_TOKEN`.

### OpenAI Python

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8790/v1",
    api_key="tentgent-local",
)

completion = client.chat.completions.create(
    model="<chat-model-ref-or-provider-model>",
    messages=[{"role": "user", "content": "Say hello in Traditional Chinese."}],
    max_tokens=64,
    temperature=0,
)

print(completion.choices[0].message.content)
```

### OpenAI JavaScript

```javascript
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "http://127.0.0.1:8790/v1",
  apiKey: "tentgent-local",
});

const completion = await client.chat.completions.create({
  model: "<chat-model-ref-or-provider-model>",
  messages: [{ role: "user", content: "Say hello in Traditional Chinese." }],
  max_tokens: 64,
  temperature: 0,
});

console.log(completion.choices[0]?.message?.content);
```

## Cloud Audio And Local Limits

The `AA==` value below is a payload placeholder, not a playable WAV. Replace it
with base64-encoded real audio before executing a cloud audio request.

Direct OpenAI cloud servers support OpenAI audio chat input and output through
`/v1/chat/completions` when the bound OpenAI model supports audio:

```bash
tentgent server run openai:gpt-audio \
  --host 127.0.0.1 \
  --port 8791
```

```bash
curl -sS http://127.0.0.1:8791/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "What is in this recording?"},
        {"type": "input_audio", "input_audio": {"data": "AA==", "format": "wav"}}
      ]
    }],
    "modalities": ["text", "audio"],
    "audio": {"voice": "alloy", "format": "wav"}
  }'
```

Daemon and local model-bound OpenAI chat routes do not implement OpenAI audio
input yet:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/chat/completions" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-audio",
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "Transcribe this."},
        {"type": "input_audio", "input_audio": {"data": "AA==", "format": "wav"}}
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

Daemon and local model-bound OpenAI chat routes do not implement OpenAI audio
output yet:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/chat/completions" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-audio",
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

## Unsupported Behavior

These requests intentionally exercise rejected fields or capabilities. Error blocks show the stable code; responses also carry a human-readable message.

OpenAI image generation with multiple images is rejected:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/images/generations" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-image-1",
    "prompt": "A small watercolor house",
    "n": 2
  }'
```

Expected stable error code:

```json
{
  "error": "unsupported_provider_field"
}
```
