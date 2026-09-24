# Provider Integrations

Use Tentgent with selected OpenAI, Anthropic/Claude, and Gemini API shapes.
These adapters cover the documented subset of each API. Consult the
[compatibility matrix](../provider-compatibility.md) before choosing fields.

| Client API | Curl, response, and SDK examples |
| --- | --- |
| OpenAI | [Chat, embeddings, images, audio, Python and JavaScript](./openai.md) |
| Anthropic / Claude | [Messages, streaming, Python](./anthropic.md) |
| Gemini | [Generate content, streaming, embeddings](./gemini.md) |

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

Set `TENTGENT_BASE_URL` to the chosen base URL before adapting an example.
Use a managed model ref for native local daemon inference; on model-bound and
cloud servers the configured target wins over a caller's compatibility model field.

## Unsupported Behavior

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

See [provider error semantics](../../contracts/provider-api-errors.md) for field,
content, operation, and capability errors. Native [reranking](../inference/embedding-rerank.md)
uses a local model ref.
