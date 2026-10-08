# Embeddings And Reranking

Embeddings turn text into vectors; reranking scores candidate documents for a query. Use models with the corresponding capability and prepare the [local runtime](../runtime.md).

## Examples And Common Operations

Run one-shot embedding inference without starting the daemon:

```bash
tentgent embed <embedding-model-ref> \
  --input "first text" \
  --input "second text" \
  --pretty
```

Run one-shot rerank inference without starting the daemon:

```bash
tentgent rerank <rerank-model-ref> \
  --query "refund policy" \
  --document "first candidate text" \
  --document "second candidate text" \
  --top-n 1 \
  --pretty
```

`tentgent embed` and `tentgent rerank` print JSON with the resolved `model_ref`
and a `data` array matching daemon `/v1/embeddings` and `/v1/rerank` responses.
They are useful for scripts and smoke tests. For repeated traffic, use daemon
REST or a direct local server with a nonzero model idle timeout to retain the loaded model between requests.

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `embed` | `-i, --input <TEXT>` | Text input to embed. Repeat this option to embed multiple strings |
| `embed, rerank` | `-H, --home <HOME>` | Optional Tentgent runtime home override |
| `embed, rerank` | `--pretty` | Pretty-print the JSON response |
| `rerank` | `-q, --query <TEXT>` | Query text to compare against the candidate documents |
| `rerank` | `-d, --document <TEXT>` | Candidate document text. Repeat this option for multiple documents |
| `rerank` | `--top-n <N>` | Return only the top N ranked documents |

## HTTP API

Start the [daemon](../daemon.md) and follow the [HTTP authentication and error rules](../api.md).

### Embeddings

```http
POST /v1/embeddings
Content-Type: application/json
```

```json
{
  "model_ref": "<embedding-model-ref>",
  "input": ["first text", "second text"]
}
```

`input` may be one string or an array of strings. The model must have
`embedding` capability metadata.

Response (illustrative vector values; dimension depends on the model):

```json
{
  "model_ref": "<resolved-embedding-model-ref>",
  "data": [
    {"index": 0, "embedding": [0.1, 0.2]},
    {"index": 1, "embedding": [0.3, 0.4]}
  ]
}
```

`data` preserves input order. Each `index` is zero-based and `embedding` is a
numeric vector. Input strings and arrays must be non-empty. The native daemon
body accepts `model_ref` and `input`; use the provider guide for additional
OpenAI-shaped fields.

### Rerank

```http
POST /v1/rerank
Content-Type: application/json
```

```json
{
  "model_ref": "<rerank-model-ref>",
  "query": "refund policy",
  "documents": ["first candidate", "second candidate"],
  "top_n": 1
}
```

`documents` must be a non-empty string array. `top_n` is optional. The model
must have `rerank` capability metadata.

Response for `top_n: 1` (illustrative score):

```json
{
  "model_ref": "<resolved-rerank-model-ref>",
  "data": [{"index": 1, "score": 0.91}]
}
```

Results are sorted by descending raw score, with the original document index
as the tie-breaker. Scores are model-dependent, not normalized probabilities.
`query` and documents must contain non-blank text; `top_n` must be between one
and the document count. Native daemon requests accept only `model_ref`,
`query`, `documents`, and optional `top_n`. Invalid native fields return HTTP
`400`; provider-shaped rerank is unsupported.

### Curl And Direct Servers

```bash
curl -sS http://127.0.0.1:8790/v1/embeddings \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<embedding-model-ref>","input":["first text","second text"]}'
curl -sS http://127.0.0.1:8790/v1/rerank \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<rerank-model-ref>","query":"refund policy","documents":["first text","second text"],"top_n":1}'
```

For a dedicated endpoint, launch the selected capability and omit `model_ref`
from the direct request because the server already binds the model:

```bash
tentgent server run <embedding-model-ref> --capability embedding --port 8781 --detach
curl -sS http://127.0.0.1:8781/v1/embeddings \
  -H 'Content-Type: application/json' -d '{"input":["first text","second text"]}'
tentgent server run <rerank-model-ref> --capability rerank --port 8782 --detach
curl -sS http://127.0.0.1:8782/v1/rerank \
  -H 'Content-Type: application/json' \
  -d '{"query":"refund policy","documents":["first text","second text"],"top_n":1}'
```

These native direct endpoints return the same `model_ref` and `data` shapes.
MLX embedding/rerank are recognized but not implemented by the bundled
Apache-licensed runtime. Inspect support before selecting a model; see the
[embedding](../../contracts/server-embedding.md) and
[rerank](../../contracts/server-rerank.md) contracts for backend limits.

## Related Guides

[Model fixtures](../model-fixtures.md) · [Model servers](../servers.md) · [OpenAI-compatible embeddings](../providers/openai.md#embeddings)
