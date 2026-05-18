# Capability-First Release Roadmap

This is the active roadmap after `v0.3.5-alpha.0`. It supersedes the older
separate release, Linux, daemon-runtime, packaging, and model-capability plans in
[archive/](./archive/).

## Direction

- Keep the product surface CLI plus daemon REST.
- Treat model storage format and serving capability as separate facts.
- Add explicit user control before relying on automatic model detection.
- Build embedding and rerank as native Tentgent capabilities before broad
  OpenAI-compatible expansion.
- Defer audio until the multimodal request and runtime contracts are specific.
- Run Apple Developer ID signing and notarization before beta or release
  candidate tags, not after the first stable release.

## Capability Vocabulary

Initial serving capabilities:

```text
chat
embedding
rerank
```

Audio remains a deferred capability family. Do not add one vague `audio`
capability to persisted metadata until the contract distinguishes at least:

```text
audio-transcription
audio-speech
```

Future audio naming should follow the endpoint and runtime shape, not only the
model file family.

## Model Classification Rules

Capability classification is evidence-based, not format-based.

- File layout can identify model format such as `safetensors`, `gguf`, or `mlx`,
  but it cannot prove whether the model is chat, embedding, rerank, or audio.
- Explicit user input has priority over automatic detection.
- Hugging Face metadata can provide a best-effort guess through `pipeline_tag`,
  repo tags, model card hints, `config.json`, and known auxiliary files.
- Ambiguous detections should stay conservative and ask for or preserve an
  explicit `--capability` value.

Capability source values:

```text
default-chat
explicit-user
huggingface-metadata
manual-update
```

Candidate command shape:

```bash
tentgent model pull BAAI/bge-small-en-v1.5 --capability embedding
tentgent model pull BAAI/bge-reranker-base --capability rerank
tentgent model import ./models/local-embed --capability embedding
```

Automatic Hugging Face detection can use these examples as hints:

- `feature-extraction`, `sentence-similarity`, `sentence-transformers`, or
  `sentence_bert_config.json` usually indicate `embedding`.
- `text-ranking`, `reranker`, or cross-encoder sequence-classification metadata
  can indicate `rerank`.
- `text-generation` or chat template metadata can indicate `chat`.

These hints are not authoritative. When confidence is low, prefer explicit user
input over guessing.

## Execution Slices

### M1: Capability Metadata Surface

- Wire explicit capability overrides into model pull and local import.
- Keep existing metadata without `model_capabilities` readable as `chat`.
- Display capabilities and source in model list, inspect, and daemon model DTOs.
- Keep `model_ref` identity unchanged when capability metadata changes.
- Update `docs/contracts/model-store.md` and command docs.

Review target:

- A user can store, inspect, and correct model capability metadata without
  starting a server.

### M2: Detection And Correction

- Add Hugging Face metadata detection as best-effort evidence.
- Record `huggingface-metadata` only when metadata is specific enough.
- Add a manual metadata update path for correcting stored capabilities.
- Warn clearly when a pull/import remains `default-chat` because detection was
  ambiguous.

Review target:

- HF pull and local import both support explicit classification, and HF pull can
  classify common embedding/rerank models without pretending all models are
  auto-detectable.

### M3: Server Compatibility Gates

- Add server capability to local server specs and daemon server DTOs.
- Reject incompatible starts and requests with clear errors:
  - chat endpoint with embedding or rerank model
  - embedding endpoint with chat or rerank model
  - rerank endpoint with chat or embedding model
- Keep chat sessions and transcript storage separate from embedding/rerank.

Review target:

- A model cannot be accidentally served through the wrong endpoint family.

### M4: Embedding MVP

- Add native `POST /v1/embeddings` through daemon REST and direct local server
  paths.
- Support string and string-array input with stable output ordering.
- Implement one local backend path first, likely sentence-transformers or a
  targeted transformers path after dependency review.
- Gate backend readiness through kernel capability state.
- Add CLI examples only after the HTTP contract is stable.

Review target:

- A managed embedding model can return vectors through the daemon without using
  chat sessions.

### M5: Rerank MVP

- Add native `POST /v1/rerank`.
- Support `query`, `documents`, and optional `top_n`.
- Return original document indexes and scores.
- Implement one local cross-encoder rerank path first.
- Gate backend readiness through kernel capability state.

Review target:

- A managed rerank model can score candidate documents and return ordered
  results through the daemon.

### M6: Audio Planning

- Define audio request and response contracts before implementation.
- Split audio capability names by workflow instead of using one broad value.
- Decide whether audio starts with transcription, speech generation, or both.
- Keep OpenAI-compatible audio rejected until kernel multimodal support exists.

Review target:

- Audio has a precise contract and capability vocabulary before runtime work
  begins.

### M7: Apple Developer ID Signing

- Run macOS Developer ID signing and notarization on prerelease artifacts before
  beta or release candidate tags.
- Keep tag-driven GitHub Releases and checksums as the release source of truth.
- Verify Gatekeeper behavior and Homebrew tap update flow.
- Do not wait for the first non-alpha release to discover signing problems.

Review target:

- A prerelease tag produces signed and notarized macOS artifacts, and the same
  pipeline is ready for beta/stable.

## Release Milestones

- Next alpha: capability metadata overrides, display, and compatibility gates.
- Later alpha: embedding MVP.
- Later alpha: rerank MVP.
- Signing prerelease: Developer ID signing and notarization pipeline passes.
- Beta/RC: embedding and rerank documented, audio still explicit as deferred
  unless its contract is implemented.

## Verification Themes

- Store tests for default, explicit, detected, and manually updated capability
  metadata.
- Import and pull tests for capability override behavior.
- Server tests for incompatible model and endpoint combinations.
- HTTP tests for embedding and rerank request validation and response ordering.
- Doctor/capability-state tests for backend readiness reporting.
- Release workflow tests or dry runs for signed macOS artifacts before beta.
