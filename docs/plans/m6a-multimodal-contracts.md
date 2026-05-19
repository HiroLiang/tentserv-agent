# M6A Multimodal Contracts

This is the focused execution plan for the first M6 slice in the
[capability-first release roadmap](./capability-first-release-roadmap.md).

Status: implemented.

## Goal

- Define native Tentgent multimodal capability vocabulary before runtime work.
- Separate parsed native media endpoints from any opaque stream proxy.
- Decide which media workflows need HTTP request/response, async job, and
  realtime streaming contracts.
- Identify small Hugging Face models that can smoke-test each planned workflow.
- Persist approved media workflow names as model metadata only.

## Documentation Boundary

- Keep runtime and payload decisions in `docs/plans/` until a native endpoint,
  proxy boundary, or versioned release surface is approved.
- Update `docs/contracts/model-store.md` and `docs/contracts/http-daemon.md`
  for the stable model metadata vocabulary.
- User-facing fixture docs may list current runnable `chat`, `embedding`, and
  `rerank` models, plus clearly marked metadata-only M6 media candidates.
- Do not add media runtime contract docs or claim media endpoints in root
  `README.md` until a stable implementation boundary exists.
- When runtime implementation starts, split stable interface text into focused
  `docs/contracts/*.md` files instead of growing this plan.

## Native Capability Vocabulary

Approved model metadata capability names:

```text
audio-transcription
audio-speech
vision-chat
image-generation
```

Rules:

- Do not persist a generic `audio`, `media`, or `multimodal` capability.
- Capability names describe the endpoint workflow, not only the file format or
  model family.
- `vision-chat` is a separate model capability from text-only `chat`; it should
  not reuse text-only message DTOs until media content parts are explicit.
- Video is not one capability. Treat future video work as workflow-specific,
  such as `video-understanding`, after payload and streaming semantics are
  clearer.
- Opaque proxy support, if added, is not a normal model serving capability.
- Hugging Face metadata detection must not infer these media capabilities yet;
  users must set them explicitly with `--capability` or `set-capability`.

## Workflow Meanings

- `audio-transcription`: audio input to text output. Optional future fields may
  include language, timestamps, chunking policy, and diarization hints.
- `audio-speech`: text input to audio output. Optional future fields may include
  voice, speaker, format, sample rate, and streaming output policy.
- `vision-chat`: image plus text input to text output. This is a chat-like
  workflow, but it must not reuse text-only chat message contracts until media
  content parts are explicit.
- `image-generation`: text, optional reference image, or mask input to image
  result output.

## Transport Shapes

M6A should classify each workflow into one or more transport shapes before
runtime code exists.

### Synchronous HTTP

Use when request and response bodies are small enough to complete in one call.

Candidate fits:

- short audio transcription smoke tests
- short text-to-speech output when the response is a small audio artifact
- single-image vision chat

### Async Media Job

Use when media payloads or generation time can exceed normal request/response
comfort.

Candidate fits:

- long audio transcription
- image generation
- video understanding
- any workflow that writes result chunks or files for later retrieval

The job contract should model:

- input spool state
- status and progress
- result spool state and read cursors
- expiration and cleanup
- structured failure state

### Realtime Duplex Streaming

Use only when the product needs low-latency bidirectional exchange.

Candidate fits:

- live speech-to-text
- voice conversation
- live video understanding

This is not required for the first native multimodal endpoint. If needed later,
evaluate WebSocket first for local daemon simplicity, then WebRTC only when
browser-native low-latency media matters.

### Opaque Stream Proxy

Use as an escape hatch for raw chunk forwarding to a selected runtime.

Rules:

- Keep it separate from native capability contracts.
- Do not imply model compatibility gates, transcript storage, OpenAI-compatible
  semantics, or payload validation.
- Treat it as a runtime tunnel with explicit resource and lifecycle limits.

## M6B Direction

M6B should implement a job-scoped media spool before realtime duplex streaming,
an opaque raw stream proxy, or a managed media artifact catalog.

Rationale:

- It gives audio, image, and future video workflows one shared place for
  job-local input chunks, result chunks, size limits, retention, and cleanup.
- It keeps long-running media generation out of ordinary request/response
  paths.
- It avoids creating permanent media objects for high-frequency tool calls
  unless a later explicit promote/save feature is approved.
- It does not prevent a later WebSocket/WebRTC or opaque stream proxy, but it
  avoids treating raw chunk forwarding as the main model-serving contract.

Recommended M6C first native endpoint candidate: `audio-transcription`, with a
small Whisper fixture and a non-realtime job spool request path.

## Candidate HF Smoke Models

These are metadata and planning-time fixtures, not product defaults.
See [../user/model-fixtures.md](../user/model-fixtures.md) for the broader
chat, embedding, rerank, and metadata-only media fixture guide.

| Workflow | Candidate | Why it is useful | Caveat |
| --- | --- | --- | --- |
| `audio-transcription` | [`openai/whisper-tiny.en`](https://huggingface.co/openai/whisper-tiny.en) | English ASR, Transformers support, safetensors, about 38M parameters. | English-only; not realtime by itself. |
| `audio-transcription` | [`openai/whisper-tiny`](https://huggingface.co/openai/whisper-tiny) | Multilingual ASR and translation-capable tiny Whisper checkpoint, about 39M parameters. | Larger language surface; still batch/chunk oriented. |
| `audio-speech` | [`facebook/mms-tts-eng`](https://huggingface.co/facebook/mms-tts-eng) | English VITS TTS, Transformers support, safetensors, about 36M parameters. | CC-BY-NC 4.0; use for local smoke tests, not as a permissive default. |
| `audio-speech` | [`suno/bark-small`](https://huggingface.co/suno/bark-small) | Transformers text-to-speech pipeline and permissive MIT license. | Heavier than MMS-TTS; better as a secondary compatibility target. |
| `vision-chat` | [`HuggingFaceTB/SmolVLM-256M-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM-256M-Instruct) | Small image+text to text model; intended for image captioning and VQA-style tasks. | Needs vision/text content-part contract; image generation is out of scope. |
| future video understanding | [`HuggingFaceTB/SmolVLM2-256M-Video-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM2-256M-Video-Instruct) | Small video-capable VLM candidate for later video contract tests. | Keep out of first native endpoint unless video payload handling is approved. |
| `image-generation` | [`hf-internal-testing/tiny-stable-diffusion-pipe`](https://huggingface.co/hf-internal-testing/tiny-stable-diffusion-pipe) | Very small Diffusers smoke fixture for parser/runtime plumbing. | Internal testing model; not a product-quality generation target. |
| `image-generation` | [`segmind/tiny-sd`](https://huggingface.co/segmind/tiny-sd) | Tiny Stable Diffusion-style model for local text-to-image smoke. | Still needs Diffusers dependency and artifact output contract. |

## Execution Slices

### 1. Capability Vocabulary Draft

- Add the candidate names to model metadata as explicit-only values.
- Decide `vision-chat` is a separate first-class capability in model metadata.
- Leave video naming deferred until transport and artifact behavior are clearer.

### 2. Payload And Spool Decisions

- Decide whether media inputs are inline base64, multipart uploads, file paths,
  job spool refs, or a combination.
- Prefer job-scoped input/result spools for larger audio, image, and video
  payloads.
- Define how local daemon storage owns temporary media and result files without
  creating a model/dataset-like media catalog.
- Define cleanup and size-limit rules before runtime implementation.

### 3. Transport Decision Matrix

- Map each workflow to sync HTTP, async job, realtime duplex, or opaque proxy.
- Choose one first native endpoint candidate.
- Decide whether M6B should be async media jobs or opaque stream proxy.

### 4. Model Metadata And Detection Rules

- Draft explicit `--capability` values for the approved native workflows.
- Keep automatic HF detection conservative.
- Classify smoke models by explicit user capability first; do not infer media
  capabilities from safetensors or Diffusers format alone.

### 5. Test Fixture Strategy

- Pick one smoke model per approved workflow.
- Prefer tiny, public, no-auth models for CI/local smoke commands.
- Record model license caveats in the plan before using them in examples.
- Do not download these models during unit tests.

### 6. Follow-Up Plan Split

- M6B should define job-scoped media input/result spools and cleanup rules.
- M6C should become the first native runtime endpoint after the contract is
  stable.
- Stable contract docs move only when M6B/M6C is approved for implementation.
- User docs may keep media fixtures only when they are clearly marked as
  metadata-only until runtime support exists.

## Non-Goals

- Do not implement audio, image, or video runtime execution.
- Do not add OpenAI-compatible media endpoints.
- Do not add realtime WebSocket/WebRTC infrastructure.
- Do not claim media endpoint or versioned behavior before it exists.

## Review Target

- M6B is selected as job-scoped media spooling with TTL, quota, and cleanup.
- M6C can choose the first native runtime endpoint from a clear matrix of
  native workflows, transport shapes, and HF smoke fixtures.
- No runtime code or user-facing claims are added before the contract boundary is
  stable.
