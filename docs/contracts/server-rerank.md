# Server Rerank

This document defines the direct local model-server rerank request contract.

## Endpoint

`POST /v1/rerank`

Request body:

```json
{
  "query": "refund policy",
  "documents": ["candidate one", "candidate two"],
  "top_n": 1
}
```

For model-bound servers launched through `tentgent server run --capability
rerank`, `model` and `model_kind` are intentionally omitted from the request.
The Rust local-server proxy forwards the request to the shared Python model
runtime daemon, which resolves the Rust-bound managed model from runtime home.

`query` must be a non-empty string. `documents` must be a non-empty string
array. `top_n` is optional and must be between `1` and the number of documents.
Unknown fields, blank strings, and non-string documents return
`400 invalid_request`.

Responses are JSON encoded as UTF-8:

```json
{
  "model_ref": "resolved-model-ref",
  "data": [
    {"index": 1, "score": 0.91},
    {"index": 0, "score": 0.22}
  ]
}
```

The response is sorted by descending score and preserves each document's
original zero-based index. Ties use the original index as the deterministic
tie-breaker.

## Capability Routing

Direct model-server rerank is stateless. The shared Python model runtime does
not read or write Tentgent session files and does not accept daemon session
fields.

The shared runtime process serves exactly one endpoint family. `--capability
rerank` serves `POST /v1/rerank`; servers launched for any other capability
reject `POST /v1/rerank` with `400 unsupported_target`. See
[server-chat.md](./server-chat.md) and [server-embedding.md](./server-embedding.md)
for sibling endpoint examples.

Cloud provider direct servers currently support only `chat`.

## Backend Status

Supported rerank model kinds:

- `transformers-rerank`
  Loads managed safetensors models with `AutoTokenizer` and
  `AutoModelForSequenceClassification`, scores query/document pairs with scalar
  logits, and returns raw backend scores.
- `mlx-rerank`
  Recognized as a runtime kind, but the Apache-licensed runtime does not
  import optional license-restricted MLX reranker packages. It returns
  `501 not_implemented`; downstream forks or external runtimes may provide a
  concrete implementation.

GGUF, Diffusers, cloud provider, and embedding backend paths are not implemented
for rerank in this contract.

## Error Mapping

- `400 invalid_request`
  Request shape is invalid.
- `400 unsupported_target`
  The request reached a server process for a different endpoint family.
- `501 not_implemented`
  The selected runtime path is recognized but not implemented.
- `500 rerank_failed`
  Backend execution failed after request validation.
