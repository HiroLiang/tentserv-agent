# Server Embedding

This document defines the direct local model-server embedding request contract.

## Endpoint

`POST /v1/embeddings`

Request body:

```json
{
  "input": ["first", "second"]
}
```

For model-bound servers launched through `tentgent server run --capability
embedding`, `model` and `model_kind` are intentionally omitted from the request.
The Rust local-server proxy forwards the request to the shared Python model
runtime daemon, which resolves the Rust-bound managed model from runtime home.

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

## OpenAI-Compatible Ingress

The same local model-bound endpoint accepts the documented OpenAI-compatible
embedding shape when a provider-style field is present:

```json
{
  "model": "text-embedding-3-small",
  "input": ["first", "second"],
  "encoding_format": "float"
}
```

`model` is accepted for client compatibility and ignored as a route selector;
the server uses the local model bound by `tentgent server run`.
`encoding_format` may be omitted or set to `float`.

Unsupported OpenAI-compatible embedding fields return stable provider
compatibility errors:

- `dimensions`
- `encoding_format = "base64"`
- `user`
- unknown fields

Output vector dimensions are selected by the bound model/runtime. Local
model-bound servers do not expose a caller-supplied dimensions override.

## Capability Routing

Direct model-server embedding is stateless. The shared Python model runtime does
not read or write Tentgent session files and does not accept daemon session
fields.

The shared runtime process serves exactly one endpoint family. `--capability
embedding` serves `POST /v1/embeddings`; servers launched for any other
capability reject `POST /v1/embeddings` with `400 unsupported_target`. See
[server-chat.md](./server-chat.md) and [server-rerank.md](./server-rerank.md)
for sibling endpoint examples.

Cloud provider embedding servers are outside this local runtime profile and
local proof scope. They use provider-hosted models and provider auth, not a
local model-store record.

## Backend Status

Supported embedding model kinds:

- `transformers-embedding`
  Loads managed safetensors models with `AutoTokenizer` and `AutoModel`,
  applies attention-mask mean pooling, and returns L2-normalized vectors.
- `mlx-embedding`
  Recognized as a runtime kind, but the Apache-licensed runtime does not
  import optional license-restricted MLX embedding packages. It returns
  `501 not_implemented`; downstream forks or external runtimes may provide a
  concrete implementation.
- `llama-cpp-embedding`
  Loads a single GGUF file with `llama-cpp-python` using `embedding=True` and
  returns the backend embedding vectors.

Local embedding server starts require a selected runtime profile for supported
local backend families:

- `local-embedding-transformers-peft-v1`
- `local-embedding-llama-cpp-v1`

The `mlx-embedding` path currently has no local runtime profile and should fail
before runtime launch when selected by `tentgent server run --capability
embedding`.

## Error Mapping

- `400 invalid_request`
  Request shape is invalid.
- `400 unsupported_target`
  The request reached a server process for a different endpoint family.
- `501 not_implemented`
  The selected runtime path is recognized but not implemented.
- `500 embedding_failed`
  Backend execution failed after request validation.
