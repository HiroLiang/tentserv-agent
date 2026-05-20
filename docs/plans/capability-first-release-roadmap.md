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
- Treat Apple Silicon local deployment as a first-class product target. When a
  practical MLX runtime exists for a media workflow, add it as a parallel local
  backend instead of leaving that workflow CPU-only on Apple Studio, Mac mini,
  or MacBook-class hardware.
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

M6C implementation record:
[m6c-audio-transcription-daemon-mvp.md](./m6c-audio-transcription-daemon-mvp.md).

M6B intentionally does not execute models. Keep the M6C-and-later plan in this
roadmap and update it as decisions change.

#### M6C: Daemon Audio Transcription Jobs

Status: implemented and smoke-tested.

- Add daemon-managed audio transcription jobs for `audio-transcription`
  safetensors ASR models.
- Use kernel job workspaces for input/result state.
- Support transcript output formats: `text`, `json`, `vtt`, and `srt`.
- Expose feature-owned result reads instead of generic workspace routes.

#### M6D: Audio Transcription File Upload Jobs

Status: implemented.

- Canonical API: `POST /v1/audio/transcriptions/job`.
- Callers send multipart `file` bytes; `curl -F file=@audio.mp3` is only
  client-side file reading shorthand, not a path-based API contract.
- The daemon persists received bytes into a job workspace, then passes the
  daemon-internal workspace path to the worker.
- Result route:
  `GET /v1/audio/transcriptions/job/{job_id}/result?cursor=0&max_chunks=32`.
- Result reads report `result_pending`, terminal job errors,
  `result_not_found`, or ready transcript bytes.
- Detailed plan:
  [m6d-audio-transcription-file-stream-job-input.md](./m6d-audio-transcription-file-stream-job-input.md).

#### M6E: Audio Transcription CLI And Large-File Hardening

Status: implemented.

- Add `tentgent transcribe /path/audio.mp3 --model-ref <model> --output transcript.txt --format text`.
- Foreground mode is not a hidden durable job.
- With `--output`, write only to the requested path and print a short
  completion message.
- Without `--output`, allow stdout for `text` and `json`; require `--output`
  for `vtt` and `srt`.
- Add `language`, `timestamps`, output-format, input-path, and output-path
  validation.
- Improve large-file safety and user guidance for ffmpeg, media format support,
  decode failures, timeout expectations, and memory/disk pressure. Treat the
  root cause as media decode and model time-window handling, not arbitrary
  byte-chunk ingestion.
- Do not add `--detach`; users can rerun foreground CLI failures, while durable
  audio jobs remain a daemon API workflow.
- Detailed plan:
  [m6e-audio-transcription-cli-and-large-file-hardening.md](./m6e-audio-transcription-cli-and-large-file-hardening.md).

#### M6F: Vision Chat Image Input

Status: implemented.

- Add native `vision-chat` typed image-plus-text requests.
- Do not reuse the text-only chat DTO for image inputs.
- Primary daemon file input is multipart image bytes. Foreground CLI uses a
  local image path and does not require the daemon.
- API: `POST /v1/vision/chat` for bounded synchronous single-image
  requests.
- Added foreground CLI `tentgent vision chat <IMAGE_PATH>` for local smoke tests
  without daemon dependency.
- Keep `POST /v1/vision/chat/job` deferred unless real local VLM smoke shows
  synchronous HTTP is not comfortable.
- Outputs: `text`, `json`, or `md`.
- Compatible OpenAI/Claude/Gemini multimodal DTOs wait until the native typed
  DTO is stable.
- Detailed plan:
  [m6f-vision-chat-image-input.md](./m6f-vision-chat-image-input.md).

#### M6G: Image Generation Jobs

Status: implemented baseline and CLI smoke-tested.

- Added `image-generation` artifact jobs.
- Canonical API: `POST /v1/images/generations/job`.
- Result/file APIs return generated `png` or `jpg` files through
  workflow-owned routes, not workspace/spool routes.
- First slice is text-to-image only with validated prompt, dimensions, seed,
  and output format.
- Implemented the first runtime path as Diffusers through the `local-model`
  Python profile, with CPU/MPS fp32 and CUDA fp16 dtype defaults.
- M6G.1 through M6G.4 follow with image LoRA, image-to-image, inpainting/masks,
  and reference image or ControlNet contracts.
- Detailed plan:
  [m6g-image-generation-jobs.md](./m6g-image-generation-jobs.md).

#### M6H: MLX Multimodal Backend Family Foundation

Status: planned.

- Detailed plan:
  [m6h-mlx-multimodal-backend-foundation.md](./m6h-mlx-multimodal-backend-foundation.md).
- Insert this before new media capability surfaces so Apple Silicon deployment
  does not lag behind the existing safetensors/Diffusers baseline.
- Split MLX runtime families instead of treating every `ModelFormat::Mlx` model
  as `mlx-lm` chat:
  - `mlx-lm` for text chat and current MLX LoRA paths
  - `mlx-vlm` for `vision-chat`
  - `mlx-audio` for ASR, audio understanding, and text-to-speech candidates
  - an MLX diffusion family, if a stable runtime is approved, for
    `image-generation`
- Add model metadata or resolver rules that can select the correct MLX runtime
  family without breaking existing `mlx-community` chat models.
- Add doctor/runtime readiness probes for any new MLX packages before exposing
  the corresponding backend.

#### M6I: MLX Vision Chat Backend

Status: planned.

- Add `vision-chat` support for MLX VLM models as a parallel backend to the
  already implemented Transformers vision path.
- Keep the native CLI and daemon API unchanged:
  `tentgent vision chat <IMAGE_PATH>` and `POST /v1/vision/chat`.
- Candidate runtime family: `mlx-vlm`.
- Candidate smoke models include small `mlx-community` VLM repos such as
  SmolVLM, LFM2-VL, or Qwen2.5-VL variants when they fit local hardware.
- Do not add OpenAI/Claude/Gemini multimodal compatibility in this slice.

#### M6J: MLX Audio Runtime Backend

Status: planned.

- Add an MLX audio backend path for `audio-transcription` before expanding
  audio workflows beyond the current ASR baseline.
- Keep the native transcription API and CLI unchanged:
  `tentgent transcribe`, `POST /v1/audio/transcriptions/job`, and result
  routes.
- Candidate runtime family: `mlx-audio`.
- Candidate smoke models include MLX Whisper ASR variants. Audio understanding
  models can be evaluated here but should not change the transcription
  contract unless a separate capability is approved.
- Evaluate whether the same runtime family is mature enough to support
  `audio-speech`; if yes, feed that directly into M6L.

#### M6K: MLX Image Generation Backend Decision

Status: planned decision and implementation split.

- Decide and, if practical, implement an MLX image-generation backend parallel
  to the current Diffusers backend.
- Keep the existing `image-generation` CLI and daemon job API unchanged.
- Candidate runtime families include DiffusionKit or other MLX Stable
  Diffusion-compatible runtimes. Do not route these through `mlx-lm`.
- If the MLX image-generation runtime is not stable enough, record the blocker
  explicitly and keep the M6G Diffusers path as the implemented baseline rather
  than blocking user-visible image generation.
- Do not start advanced image-generation sub-slices such as image LoRA,
  image-to-image, inpainting, reference images, or ControlNet until the Apple
  Silicon backend decision is recorded.

#### M6L: Audio Speech Jobs

Status: planned.

- Add `audio-speech` artifact jobs for text-to-speech.
- Canonical API: `POST /v1/audio/speech/job`.
- First output format should be `wav`; `flac` can follow if supported.
- `mp3` waits until encoder dependency and licensing boundaries are approved.
- Voice/language selection must be model-aware and fail early when unsupported.
- Include both Transformers and MLX runtime candidates when practical, rather
  than shipping a speech path that is unnecessarily CPU-only on Apple Silicon.
- Realtime speech streaming is out of scope for this slice.

#### M6M: Video Understanding

Status: planned, contract first.

- Add video plus prompt to text/JSON/Markdown understanding jobs only after the
  payload and result contract is approved.
- Candidate API: `POST /v1/video/understanding/job`.
- Primary input should be multipart video bytes, with trusted local path only
  as a local/debug fallback if kept.
- Workers must decode/sample frames or clips with bounds such as `fps`, max
  frame count, max pixels, and timeout.
- Do not load whole videos into memory.
- Keep this contract-only if no practical small local model/runtime fixture is
  approved.

#### M6N: Video Generation Artifact Decision

Status: decision slice, not implementation by default.

- Decide whether local video generation belongs in this release line.
- If approved, it must be job-only and produce playable files such as `mp4` or
  `webm`.
- Define encoder dependencies, hardware expectations, temporary disk usage,
  cleanup retention, and output artifact shape before runtime work.
- Raw frames are debug/advanced artifacts only.
- If no-go, keep `video-generation` out of accepted capability values until a
  later milestone.

#### M6O: Media Serving And Runtime Stream Proxy Decision

Status: planned decision and implementation split.

- Decide which media capabilities get long-lived `tentgent server` routes and
  which remain durable job workflows.
- Server route families are selected by capability. Chat servers expose chat
  routes, embedding servers expose `/v1/embeddings`, rerank servers expose
  `/v1/rerank`, and audio transcription servers may expose
  `/v1/audio/transcriptions`.
- Unsupported route families should return `404` or endpoint-specific
  unsupported errors.
- Direct serving is for warm models and bounded requests. Long-running
  generation, very large uploads, resumable work, and durable artifacts should
  remain job workflows.
- Opaque backend proxying is advanced later work and must not be the default
  user API.

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
  rerank MVP, M6A media metadata vocabulary, audio transcription, and native
  single-image vision chat are implemented and documented.
- Multimodal planning follow-up: kernel-owned job workspaces are implemented
  before native runtime work; M6C audio transcription, M6D file-upload job
  intake, M6E foreground transcription CLI, M6F native vision chat, and M6G
  image-generation jobs are implemented; M6H-and-later prioritizes MLX media
  backend parity for Apple Silicon before opening additional media surfaces by
  CLI, output format, server, and transport shape.
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
- MLX media backend tests for runtime-family selection, Apple Silicon readiness
  diagnostics, and parity with native audio, vision, and image APIs.
- Doctor/capability-state tests for backend readiness reporting.
- Release workflow tests or dry runs for signed macOS artifacts before beta.
