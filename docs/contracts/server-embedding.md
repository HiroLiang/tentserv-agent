# Server Embedding

This document defines the direct Python server embedding request contract.

## Endpoint

`POST /v1/embeddings`

Request body:

```json
{
  "input": ["first", "second"]
}
```

`input` accepts either one string or a non-empty string array. Empty arrays,
empty strings, unknown fields, and non-string values return `400 invalid_request`.

Responses are JSON encoded as UTF-8:

```json
{
  "model_ref": "resolved-model-ref",
  "data": [
    {"index": 0, "embedding": [0.1, 0.2]},
    {"index": 1, "embedding": [0.3, 0.4]}
  ]
}
```

The response preserves input order. Embeddings are JSON numbers, not strings.

## Capability Routing

Direct model-server embedding is stateless. The Python server runtime does not
read or write Tentgent session files and does not accept daemon session fields.

The process serves exactly one endpoint family:

- `--capability chat` serves `POST /v1/chat` and rejects `POST /v1/embeddings`
  with `400 unsupported_target`.
- `--capability embedding` serves `POST /v1/embeddings` and rejects
  `POST /v1/chat` with `400 unsupported_target`.
- `--capability rerank` is not implemented.

Cloud provider direct servers currently support only `chat`.

## Backend Status

The first embedding backend is `transformers-peft` for managed safetensors
models. It loads `AutoTokenizer` and `AutoModel`, applies attention-mask mean
pooling, and returns L2-normalized vectors.

GGUF, MLX, cloud provider, and rerank embedding paths are not implemented in
this contract.

## Error Mapping

- `400 invalid_request`
  Request shape is invalid.
- `400 unsupported_target`
  The request reached a server process for a different endpoint family.
- `501 not_implemented`
  The selected runtime path is recognized but not implemented.
- `500 embedding_failed`
  Backend execution failed after request validation.
