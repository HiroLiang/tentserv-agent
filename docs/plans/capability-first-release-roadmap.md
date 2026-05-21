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
- Treat M6C through M6J as completed media capability slices while M6K and later
  finish the remaining media workflow and serving decisions.
- Separate native parsed media endpoints from any opaque stream-in/stream-out
  runtime proxy before implementation starts.
- Treat Apple Silicon local deployment as a first-class product target. When a
  practical MLX runtime exists for a media workflow, add it as a parallel local
  backend instead of leaving that workflow CPU-only on Apple Studio, Mac mini,
  or MacBook-class hardware.
- Keep full cross-runtime compatibility, durable runtime proof storage,
  model/LoRA adapter compatibility management, dynamic runtime transduction,
  shared compatibility registry, and broad model resource coordination out of
  the M6-to-M7 release track. Track those as post-M7 architecture work.
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

Status: implemented and CLI smoke-tested.

- Added `image-generation` artifact jobs.
- Canonical API: `POST /v1/images/generations/job`.
- Result/file APIs return generated `png` or `jpg` files through
  workflow-owned routes, not workspace/spool routes.
- First slice is text-to-image only with validated prompt, dimensions, seed,
  and output format.
- Implemented the first runtime path as Diffusers through the `local-model`
  Python profile, with CPU/MPS fp32 and CUDA fp16 dtype defaults.
- Advanced image workflows are not part of M6G. Image LoRA, image-to-image,
  inpainting/masks, and reference image or ControlNet contracts move to later
  standalone slices after the MLX image backend decision.
- Detailed plan:
  [m6g-image-generation-jobs.md](./m6g-image-generation-jobs.md).

#### M6H: MLX Multimodal Backend Family Foundation

Status: implemented foundation.

- Detailed plan:
  [m6h-mlx-multimodal-backend-foundation.md](./m6h-mlx-multimodal-backend-foundation.md).
- Insert this before new media capability surfaces so Apple Silicon deployment
  does not lag behind the existing safetensors/Diffusers runtime paths.
- Split MLX runtime families instead of treating every `ModelFormat::Mlx` model
  as `mlx-lm` chat:
  - `mlx-lm` for text chat and current MLX LoRA paths
  - `mlx-vlm` for `vision-chat`
  - `mlx-audio` for ASR, audio understanding, and text-to-speech candidates
  - an MLX diffusion family, if a stable runtime is approved, for
    `image-generation`
- Added model metadata and resolver rules that select or reject by MLX runtime
  family without breaking existing `mlx-community` chat models.
- Python runtime records and router now read `mlx_runtime_family`; MLX media
  families return explicit planned-backend errors until their dedicated backend
  slice implements them.
- Doctor/runtime readiness probes for new MLX packages remain part of the
  corresponding backend implementation slices.

#### M6I: MLX Vision Chat Backend

Status: implemented and smoke-tested.

- Detailed plan:
  [m6i-mlx-vision-chat-backend.md](./m6i-mlx-vision-chat-backend.md).
- Added `vision-chat` support for MLX VLM models as a parallel backend to the
  already implemented Transformers vision path.
- Keep the native CLI and daemon API unchanged:
  `tentgent vision chat <IMAGE_PATH>` and `POST /v1/vision/chat`.
- Candidate runtime family: `mlx-vlm`.
- Primary smoke candidate:
  `mlx-community/SmolVLM-256M-Instruct-bf16`.
- Fallback candidates include `mlx-community/LFM2-VL-450M-4bit` and
  `mlx-community/Qwen2.5-VL-3B-Instruct-4bit` when they fit local hardware and
  pass runtime API checks.
- Added a dedicated MLX VLM backend readiness probe instead of treating this as
  generic `mlx-lm`.
- Do not add OpenAI/Claude/Gemini multimodal compatibility in this slice.

#### M6J: MLX Audio Runtime Backend

Status: implemented and smoke-tested.

- Detailed plan:
  [m6j-mlx-audio-runtime-backend.md](./m6j-mlx-audio-runtime-backend.md).
- Added an MLX audio backend path for the existing `audio-transcription`
  workflow before expanding audio workflows beyond the current ASR baseline.
- Kept the native transcription API and CLI unchanged:
  `tentgent transcribe`, `POST /v1/audio/transcriptions/job`, and result
  routes.
- Runtime family: `mlx-audio`.
- Added a dedicated audio backend selection path rather than treating MLX audio
  as generic `mlx-lm`:
  - `safetensors` remains `transformers-asr`
  - `mlx + mlx_runtime_family = mlx-audio` routes to the new MLX audio backend
  - other MLX families are rejected for audio transcription
- Smoke-tested with `mlx-community/whisper-tiny-asr-fp16` through CLI text,
  CLI JSON/VTT timestamp output, and daemon multipart upload/result routes.
- Older `mlx-community/whisper-tiny-mlx` and
  `mlx-community/whisper-tiny-fp16` pulled successfully but failed with current
  `mlx-audio` because they lack Hugging Face processor metadata required by
  the package's Whisper loader.
- Audio understanding and text-to-speech models were left for later slices;
  they must not change the transcription contract unless a separate capability
  is approved.
- Evaluate whether the same runtime family is mature enough to support future
  `audio-speech`; if yes, feed that into M6P instead of broadening M6J.
- Did not add a default same-model lock for read-only transcription inference.
  Future model resource coordination should focus on mutation/exclusive-state
  tasks and optional operator-configured runtime capacity limits.

#### M6K: MLX Image Generation Backend

Status: implemented and smoke-tested.

- Detailed plan:
  [m6k-mlx-image-generation-backend-decision.md](./m6k-mlx-image-generation-backend-decision.md).
- Implemented an MFLUX-backed MLX image-generation backend parallel to the
  current Diffusers backend.
- Keep the existing `image-generation` CLI and daemon job API unchanged.
- Runtime family: `mlx-diffusion`.
- Selected runtime: MFLUX Flux-family text-to-image on Apple Silicon.
- Current public smoke candidate:
  `mlx-community/Flux-1.lite-8B-MLX-Q4`.
- Smoke-tested through foreground CLI and daemon image-generation job routes
  with short ref `96fdb6180caa`.
- Keep the completed M6G Diffusers path as the small cross-platform image
  baseline.
- Do not start advanced image-generation slices M6L through M6O until the Apple
  Silicon backend decision is recorded.

#### M6L: Image Generation LoRA

Status: implemented and unit-tested; public LoRA fixture smoke pending.

- Detailed plan:
  [m6l-image-generation-lora.md](./m6l-image-generation-lora.md).
- Add image-generation adapter selection for Diffusers pipelines and any
  approved MLX image backend.
- Extend the existing native `tentgent image generate` command and
  `POST /v1/images/generations/job` route with one optional adapter reference
  and LoRA scale instead of adding a separate image LoRA endpoint.
- Generalize adapter compatibility so image-generation adapters do not reuse the
  current chat-only LoRA assumption.
- Add image LoRA adapter metadata for target capability, backend support,
  selected weight file, trigger-word hints, and optional recommended scale.
- Expose that metadata through CLI adapter import/pull and daemon adapter
  import/pull, including detached job variants.
- Support Diffusers image LoRA and the approved MFLUX `mlx-diffusion` path when
  a compatible local managed adapter file can be proven.
- Current implementation passes managed local adapter weight paths through both
  Diffusers and MFLUX. A small public LoRA fixture still needs to be pinned for
  repeatable real-model smoke.
- Keep multi-LoRA stacking, image LoRA training, prompt auto-injection,
  image-to-image, masks, reference images, and ControlNet out of this slice.

#### M6M: Image-To-Image

Status: implemented and unit-tested; real-model smoke pending. Details in
[m6m-image-to-image.md](./m6m-image-to-image.md).

- Add one-input-image transforms under the existing `image-generation`
  capability: one input image plus prompt produces one output image artifact.
- Add foreground CLI `tentgent image transform` with local input path, protected
  output path, optional negative prompt, optional one image LoRA adapter, and
  Diffusers-compatible `strength` validation.
- Add native daemon multipart route `POST /v1/images/transforms/job`; daemon
  receives image bytes, writes them into the job workspace, starts the worker
  only after upload persistence, and exposes workflow-owned result file routes.
- Keep generated output routes aligned with M6G while keeping transform routes
  distinct from text-to-image generation routes.
- Implement Diffusers image-to-image through the local Python runtime. Implement
  the MFLUX image-to-image path only if the installed runtime exposes a stable
  local-model/local-image API; otherwise return a clear unsupported-backend
  error and keep the gap documented.
- Keep masks, inpainting, reference images, ControlNet, multi-image input,
  OpenAI-compatible image edits APIs, and direct server routes out of this
  slice.

#### M6N: Inpainting And Masks

Status: implemented and unit-tested; real-model smoke pending. Details in
[m6n-inpainting-and-masks.md](./m6n-inpainting-and-masks.md).

- Add masked inpainting under the existing `image-generation` capability:
  one base image plus one mask plus prompt produces one output image artifact.
- Add foreground CLI `tentgent image inpaint` with local base-image path,
  local mask-image path, protected output path, optional negative prompt,
  optional one image LoRA adapter, and Diffusers-compatible `strength`
  validation.
- Add native daemon multipart route `POST /v1/images/inpaint/job`; daemon
  receives base image and mask bytes, writes both into the job workspace,
  starts the worker only after upload persistence, and exposes workflow-owned
  result file routes.
- Define mask semantics explicitly: white pixels repaint and black pixels keep
  the original image. Runtime normalizes masks to binary grayscale and rejects
  image/mask dimension mismatches before model loading.
- Implement Diffusers inpainting through `AutoPipelineForInpainting`.
- Implement the MFLUX Flux Fill path with a compatibility guard that rejects
  non-fill-looking MLX diffusion models before runtime execution.
- Keep mask inversion, mask blur/feather controls, automatic segmentation,
  reference images, ControlNet, multi-image input, OpenAI-compatible image
  edits APIs, and direct server routes out of this slice.

#### M6O: Reference Images And ControlNet

Status: implemented, unit-tested, and tiny-fixture smoke-tested. Details in
[m6o-reference-images-and-controlnet.md](./m6o-reference-images-and-controlnet.md).

- Added a typed controlled image-generation workflow after simpler image input
  contracts stabilized.
- Public workflow: `tentgent image control` and
  `POST /v1/images/control/job`, with one prompt, one typed control image, one
  managed ControlNet-style control asset, and one output image.
- Treats the base image model, optional image LoRA adapter, and ControlNet-style
  control asset as separate compatibility surfaces.
- Keeps ControlNet assets in managed adapter/control metadata rather than
  pretending they are normal base image models or LoRA adapters.
- Treats generic reference-image composition as model/pipeline-specific until a
  concrete contract is proven; do not add a generic `reference_image` field in
  this slice.
- Does not merge controlled generation into the baseline text-to-image route
  without this typed request shape.
- Verified daemon smoke with
  `hf-internal-testing/tiny-stable-diffusion-pipe-no-safety` plus
  `hf-internal-testing/tiny-controlnet` at `64x64`, `2` steps.

#### M6P: Audio Speech Jobs

Status: implemented for the Transformers text-to-speech path. Details in
[m6p-audio-speech-jobs.md](./m6p-audio-speech-jobs.md).

- Added `audio-speech` artifact jobs for text-to-speech.
- Canonical API: `POST /v1/audio/speech/job`.
- Foreground CLI: `tentgent speak`.
- Result API: `GET /v1/audio/speech/job/{job_id}/result`.
- First output format is `wav`; `flac` can follow if supported.
- `mp3` waits until encoder dependency and licensing boundaries are approved.
- Voice/language selection must be model-aware and fail early when unsupported.
- Realtime speech streaming is out of scope for this slice.
- First reliable backend path is Transformers `text-to-speech`; MLX audio TTS
  is exposed as a clear planned-backend error until the installed `mlx-audio`
  API and a small fixture can be verified.
- Smoke-tested with `facebook/mms-tts-eng` through both `tentgent speak` and
  daemon `POST /v1/audio/speech/job`; both produced PCM WAV results.

#### M6Q: Video Understanding

Status: planned, contract first. Details in
[m6q-video-understanding-jobs.md](./m6q-video-understanding-jobs.md).

- Add a dedicated `video-understanding` capability rather than overloading
  `vision-chat`.
- Public workflow: `tentgent video understand` and
  `POST /v1/video/understanding/job`.
- Primary daemon input is multipart video bytes. CLI input is a local path and
  runs in the foreground.
- Result API: `GET /v1/video/understanding/job/{job_id}/result`.
- Output formats are text-like: `text`, `md`, and `json`.
- Use a dedicated video upload cap, `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`, instead
  of raising the generic image/audio media upload cap.
- Workers must decode/sample frames or clips with bounds such as sample FPS,
  max frame count, max frame edge, clip start/duration, and timeout.
- Do not load whole videos into memory.
- Keep this contract-only if no practical small local model/runtime fixture is
  approved. Primary fixture candidate is
  `HuggingFaceTB/SmolVLM2-256M-Video-Instruct`.

#### M6R: Video Generation Artifact Decision

Status: implemented as an internal contract slice. Details in
[m6r-video-generation-artifact-decision.md](./m6r-video-generation-artifact-decision.md).

- Added the kernel-internal video-generation artifact contract only:
  output format, prompt, dimensions, bounded generation options, text-to-video
  input marker, artifact plan, and artifact metadata.
- Current fixture conclusion: no suitable small real Hugging Face fixture is
  approved for public local smoke testing. Tiny dummy pipelines may only prove
  plumbing and must not be presented as usable video-generation support.
- If a future fixture is approved, the public workflow must be job-only and
  produce playable files such as `mp4` or `webm`.
- Encoder dependencies, hardware expectations, temporary disk usage, cleanup
  retention, and output artifact shape are captured before runtime work.
- Raw frames are debug/advanced artifacts only.
- Keep `video-generation` out of accepted public capability values, user-facing
  CLI commands, and daemon routes until a real fixture gate passes.

#### M6S: Media Serving And Runtime Stream Proxy Decision

Status: deferred to post-M7. Details in
[m6s-media-serving-and-runtime-stream-proxy-decision.md](./m6s-media-serving-and-runtime-stream-proxy-decision.md).

- Do not add new media direct server routes before M7.
- Keep existing server route families for `chat`, `embedding`, and `rerank`.
- Keep `vision-chat` and `audio-transcription` direct server wrapping as
  post-M7 candidates, not M6 implementation work.
- Keep `audio-speech`, `image-generation`, `video-understanding`, and future
  `video-generation` as durable job workflows.
- Move media-serving wrappers, route-family expansion, and runtime stream proxy
  decisions into the post-M7 runtime compatibility architecture track.
- Opaque backend proxying is advanced later work and must not be added before
  M7.

Review target:

- The remaining M6 work has workflow-specific API, CLI, upload, output-format,
  server, and transport decisions before release engineering starts.

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
  rerank MVP, M6A media metadata vocabulary, audio transcription, audio
  speech, native single-image vision chat, and image generation/editing
  workflows are implemented and documented.
- M6 in progress: kernel-owned job workspaces, file-upload job intake,
  foreground media CLI commands, native vision chat, text-to-image jobs,
  image-to-image, inpainting, ControlNet-style image control, MLX
  runtime-family metadata, MLX vision chat, MLX audio transcription, and MFLUX
  image generation have completed their first implementation slices.
- M6 remaining before M7: no product workflow implementation blockers are
  currently planned. Video generation remains internal/test-only, and media
  serving/runtime stream proxy work is deferred to post-M7 architecture.
- M7: Developer ID signing and notarization pipeline for prerelease macOS
  artifacts.
- Post-M7 architecture work:
  [post-m7-runtime-compatibility-architecture.md](./post-m7-runtime-compatibility-architecture.md)
  tracks full model compatibility, LoRA adapter compatibility management,
  SQLite-backed metadata/proof storage, dynamic runtime transduction,
  compatibility probe/cache, optional shared registry, resource coordination,
  and conversion boundaries. It is not part of the current M6-to-M7 release
  track, and should be renamed when initialized after M7.
- Beta/RC: chat, embedding, rerank, and the completed M6 multimodal surfaces
  are documented with smoke evidence and known limits.

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
