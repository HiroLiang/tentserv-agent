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
- Treat M6 as multimodal and streaming-boundary planning, not an audio-only
  implementation slice.
- Separate native parsed media endpoints from any opaque stream-in/stream-out
  runtime proxy before implementation starts.
- Run Apple Developer ID signing and notarization before beta or release
  candidate tags, not after the first stable release.

## Capability Vocabulary

Initial serving capabilities:

```text
chat
embedding
rerank
```

Multimodal capabilities remain deferred. Do not add one vague `audio`, `media`,
or `multimodal` capability to persisted metadata until the contract distinguishes
the concrete workflow shape. Audio should distinguish at least:

```text
audio-transcription
audio-speech
```

Future image and video names should follow the same rule: name the endpoint and
runtime workflow, not only the model file family. An opaque runtime stream proxy,
if added, is an escape hatch for raw byte or chunk forwarding and should not be
stored as a normal model serving capability.

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

Detailed plan: [m2-model-capability-detection-and-correction.md](./m2-model-capability-detection-and-correction.md).

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

Detailed plan: [m3-server-compatibility-gates.md](./m3-server-compatibility-gates.md).

- Add server capability to local server specs and daemon server DTOs.
- Reject incompatible starts and requests with clear errors:
  - chat endpoint with embedding or rerank model
  - embedding endpoint with chat or rerank model
  - rerank endpoint with chat or embedding model
- Keep chat sessions and transcript storage separate from embedding/rerank.

Review target:

- A model cannot be accidentally served through the wrong endpoint family.

### M4: Embedding MVP

Detailed plan: [m4-embedding-mvp.md](./m4-embedding-mvp.md).

- Status: implemented.
- Added native `POST /v1/embeddings` through daemon REST and direct local server
  paths.
- Supported string and string-array input with stable output ordering.
- Implemented the first local backend path as safetensors via the existing
  `transformers-peft` local-model profile and `AutoModel` mean pooling.
- Added embedding backend readiness to kernel capability state.
- Added CLI and curl examples after the HTTP contract stabilized.

Review target:

- A managed embedding model can return vectors through the daemon without using
  chat sessions.

### M5: Rerank MVP

Detailed plan: [m5-rerank-mvp.md](./m5-rerank-mvp.md).

- Status: implemented.
- Added native `POST /v1/rerank` through daemon REST and direct local server
  paths.
- Added CLI one-shot `tentgent embed` and `tentgent rerank` paths for scripts
  and smoke tests that do not need a running daemon.
- Supported `query`, `documents`, and optional `top_n`.
- Returned original document indexes and scores ordered by relevance.
- Implemented the first local backend path as safetensors via the existing
  `transformers-peft` local-model profile and sequence classification.
- Added rerank backend readiness to kernel capability state.

Review target:

- A managed rerank model can score candidate documents and return ordered
  results through the daemon.

### M6A: Multimodal Contracts

Detailed plan: [m6a-multimodal-contracts.md](./m6a-multimodal-contracts.md).

- Added metadata-only model capability values for `audio-transcription`,
  `audio-speech`, `vision-chat`, and `image-generation`.
- Kept media capability inference explicit-only; Hugging Face metadata
  detection still classifies only confident chat, embedding, and rerank
  evidence.
- Split media capability names by workflow instead of using one broad value.
- Identified small Hugging Face smoke models for each candidate workflow.
- Chose kernel-owned job workspaces as the M6B runtime boundary.
- Kept the opaque proxy contract separate from native capability contracts: it
  may forward bytes or chunks without parsing model-specific payloads, but it
  should not imply validation, compatibility gates, transcript state, or
  OpenAI-compatible semantics.
- Keep OpenAI-compatible audio, image, and video routes rejected until kernel
  multimodal support exists.

Review target:

- M6A has a precise native multimodal metadata vocabulary plus an explicit
  decision that M6B starts with kernel job workspaces, not an opaque streaming
  proxy.

### M6B: Kernel Job Workspace Boundary

Detailed plan:
[m6b-kernel-job-workspace-foundation.md](./m6b-kernel-job-workspace-foundation.md).

- Status: needs kernel refactor before M6C.
- Move job identity, status, workspace, chunk IO, result files, and cleanup
  semantics into `tentgent-kernel`.
- Keep daemon worker scheduling, shutdown coordination, and Python runtime
  invocation in `tentgent-daemon`.
- Keep daemon in-flight job management in `tentgent-daemon`; kernel must not
  hold active task or detached process handles.
- Provide kernel ports for opening job workspaces, listing jobs, inspecting job
  status, canceling/deleting jobs, removing workspaces, chunked read/write,
  result file listing, and result file reads.
- Keep daemon lifecycle and model-bound server lifecycle out of the job catalog.
- Use `job_id` as the workflow handle instead of creating a managed
  `media_ref` catalog.
- Add explicit cleanup rules and retention-aware shutdown sweeps to protect SSD
  usage.
- Keep future CLI media model commands simple: users provide an input file and
  output path while the CLI hides job/workspace details by default.
- Keep opaque stream proxy work separate from native media endpoint contracts.
- Leave model runtime execution to M6C.

Review target:

- Kernel job/workspace ports are the shared foundation before any audio, image,
  or video model endpoint exists.

### M6C And Later: Media Runtime Workflows

Detailed plan:
[m6c-through-m6h-media-runtime-roadmap.md](./m6c-through-m6h-media-runtime-roadmap.md).

M6B intentionally does not execute models. The follow-up M6 slices should stage
the native workflow work in this order:

- M6C: daemon audio transcription jobs using `audio-transcription` models,
  kernel job workspaces, transcript output formats, and feature-specific result
  retrieval. This slice is implemented and smoke-tested.
- M6D: audio transcription file-stream job input through the functional
  `POST /v1/audio/transcriptions/job` route, while workspace chunks remain
  internal. This slice is implemented. Detailed plan:
  [m6d-audio-transcription-file-stream-job-input.md](./m6d-audio-transcription-file-stream-job-input.md).
- M6E: CLI foreground file-to-output wrapper for transcription plus audio
  large-file decode/window hardening.
- M6F: vision chat with explicit typed image-plus-text DTOs and
  text/JSON/Markdown outputs.
- M6G: image generation jobs that turn prompts into `png` or `jpg` result
  files.
- M6H: audio speech jobs that turn text into `wav` or later approved audio
  result files.
- M6I: video understanding jobs that sample frames/clips from complete logical
  video files without loading whole videos into memory.
- M6J: video generation artifact decision and, if approved, job-only playable
  video outputs.
- M6K: media serving and runtime stream proxy decision for long-lived
  capability-native server routes versus durable job workflows.

Review target:

- The remaining M6 work has workflow-specific API, CLI, upload, output-format,
  server, and transport decisions before runtime implementation starts.

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

- Current alpha line: capability metadata, compatibility gates, embedding MVP,
  rerank MVP, and M6A media metadata vocabulary are implemented and documented.
- Multimodal planning follow-up: kernel-owned job workspaces are implemented
  before native runtime work, M6C audio transcription and M6D file-upload job
  intake are implemented, and M6E-and-later stages the remaining media
  workflows by CLI, output format, server, and transport shape.
- Signing prerelease: Developer ID signing and notarization pipeline passes.
- Beta/RC: chat, embedding, and rerank are documented; multimodal endpoints
  remain explicitly deferred unless their contracts and runtime paths are
  implemented.

## Verification Themes

- Store tests for default, explicit, detected, and manually updated capability
  metadata.
- Import and pull tests for capability override behavior.
- Server tests for incompatible model and endpoint combinations.
- HTTP tests for embedding and rerank request validation and response ordering.
- Metadata tests for explicit-only M6A media capability values.
- Job workspace tests for path input, upload input, chunk IO, result file list
  and reads, quota, cleanup, and fake-worker handoff.
- Doctor/capability-state tests for backend readiness reporting.
- Release workflow tests or dry runs for signed macOS artifacts before beta.
