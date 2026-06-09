# Provider-Compatible Examples

These examples show how to call the provider-shaped Tentgent routes that are
currently implemented. They are convenience adapters, not full upstream API
parity. Check [provider-compatibility.md](./provider-compatibility.md) before
depending on a provider field or endpoint family.

## Base URLs

Use one of these serving surfaces:

| Surface | Start command | Example base URL | Model selection |
| --- | --- | --- | --- |
| Daemon compatibility adapters | `tentgent daemon start --host 127.0.0.1 --port 8790` | `http://127.0.0.1:8790` | Daemon routes use request model fields or path models where documented. |
| Local model-bound server | `tentgent server run <model-ref> --host 127.0.0.1 --port 8780` | `http://127.0.0.1:8780` | The server uses the local model from launch; provider-shaped `model` fields are accepted for compatibility and ignored. |
| Direct cloud provider server | `tentgent server run openai:<model> --host 127.0.0.1 --port 8783` | `http://127.0.0.1:8783` | The server uses the provider model from launch; caller model fields are not route selectors. |

The examples below assume a loopback daemon or server without
`TENTGENT_DAEMON_TOKEN`. If a daemon token is enabled, add:

```bash
-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

## OpenAI-Compatible Curl

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

Direct cloud image-generation servers and local image-generation model-bound
servers also expose `/v1/images/generations`. In direct cloud mode, send
`prompt` and optional `size`; the bound provider model from
`tentgent server run <provider>:<model>` is used. In local model-bound mode,
the bound local image-generation model is used and caller provider selection is
rejected.

Provider-compatible image generation is intentionally narrower than OpenAI:
`n` and `response_format` are rejected, and responses are always
OpenAI-shaped `b64_json` envelopes.

## Claude-Compatible Curl

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

## Gemini-Compatible Curl

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
translate text and `inlineData` image parts for compatible models. Daemon and
local model-bound Gemini routes are text-only today.

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

Daemon and local model-bound Gemini routes reject inline image parts until the
local multimodal context pipeline exists:

```bash
curl -sS "$TENTGENT_BASE_URL/v1beta/models/gemini-2.5-flash:generateContent" \
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
Official Gemini `embedContent`, rerank, audio, and broader multimodal local
Gemini ingress remain future compatibility work.

## SDK Base URL Examples

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

## Unsupported Behavior Examples

Provider-compatible routes return stable `400` error codes for known
unsupported fields, content, operations, and capabilities. See
[provider-api-errors.md](../contracts/provider-api-errors.md) for the full
error contract.

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

Anthropic embeddings are not implemented. Anthropic's own docs recommend
Voyage AI for embeddings, but Voyage is not treated as a Claude-compatible
provider in Tentgent until it is added as a separate provider family.

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

Provider-compatible rerank is not implemented. Native `/v1/rerank` uses
`model_ref`, not provider `model` selectors:

```bash
curl -sS "$TENTGENT_BASE_URL/v1/rerank" \
  -H 'Content-Type: application/json' \
  -d '{
    "provider": "openai",
    "model": "text-rerank-001",
    "query": "refund policy",
    "documents": ["refunds are processed in 3 days"],
    "top_n": 1
  }'
```

Expected stable error code:

```json
{
  "error": "unsupported_provider_capability"
}
```
