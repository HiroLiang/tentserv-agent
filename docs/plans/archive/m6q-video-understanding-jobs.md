# M6Q Video Understanding Jobs

Status: planned execution slice, contract first. Runtime implementation should
proceed only if a practical local fixture can be verified without loading whole
videos into memory.

M6Q adds the first native video-understanding workflow: one video plus one
prompt produces one text-like result. It should reuse the daemon job workspace
foundation and the vision-chat runtime lessons, but it must keep video payload,
decode, sampling, and output contracts explicit.

## Goal

Allow users to ask one question about one local video:

```bash
tentgent video understand /path/to/video.mp4 \
  --model-ref <video-understanding-model-ref> \
  --prompt "Describe the main action in this clip." \
  --output answer.md \
  --format md
```

HTTP integrations should use a daemon multipart job route:

```http
POST /v1/video/understanding/job
Content-Type: multipart/form-data
```

The result should be downloaded through a workflow-owned route:

```http
GET /v1/video/understanding/job/{job_id}/result
```

## Depends On

- [M6A multimodal contracts](./m6a-multimodal-contracts.md)
- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6F vision chat image input](./m6f-vision-chat-image-input.md)
- [M6H MLX multimodal backend foundation](./m6h-mlx-multimodal-backend-foundation.md)

## Scope

In scope:

- New model capability string: `video-understanding`.
- One local model with `video-understanding` capability.
- One video input and one text prompt.
- Foreground CLI execution through kernel use cases.
- Daemon multipart upload to a job-owned workspace.
- Text-like result formats: `text`, `md`, and `json`.
- Bounded video decode and sampling controls.
- Transformers video-capable VLM runtime path if the fixture works.
- Clear early failures for unsupported model capability, unsupported backend,
  unsupported input media type, unsafe output paths, decode failures, sampling
  limit violations, and unavailable runtime dependencies.

Out of scope:

- Video generation.
- Video editing or video-to-video.
- Live camera input.
- Realtime streaming understanding.
- Speech transcription from video audio.
- Returning raw frames or sampled-frame debug artifacts by default.
- Multi-video input.
- OpenAI, Claude, or Gemini multimodal compatibility routes.
- Direct `tentgent server` video routes.
- Generic spool/upload/result workspace APIs.

## Product Decisions

- Add a dedicated `video-understanding` capability instead of reusing
  `vision-chat`. Some VLMs can do both, but video intake has separate decode
  and sampling semantics.
- Add a foreground CLI command group:
  `tentgent video understand`.
- Add a native daemon route:
  `POST /v1/video/understanding/job`.
- Add a workflow-owned result route:
  `GET /v1/video/understanding/job/{job_id}/result`.
- Daemon input is multipart file bytes. It must not trust client-local paths.
- CLI input is a local path. CLI does not create daemon jobs.
- The daemon writes the uploaded video to the job workspace before runtime
  execution starts.
- The runtime receives a local workspace video path and must use bounded decode
  or bounded frame sampling. It must not read the whole video into memory.
- Video upload size must use its own cap, not the generic media file cap. Add
  `TENTGENT_VIDEO_UPLOAD_MAX_BYTES` for video file parts, with a default large
  enough for local smoke tests such as `test-data/test_video.mp4`.
  `TENTGENT_MEDIA_UPLOAD_MAX_BYTES` remains the generic image/audio multipart
  cap and should not be raised just to accommodate video.
- The first supported input containers should be `mp4`, `mov`, and `webm`
  when the runtime can decode them.
- Decoder ownership is split:
  - Tentgent owns Python runtime dependencies it can bootstrap, such as
    `decord` when the verified backend requires it.
  - The operator owns system/native media decoders such as `ffmpeg`/LibAV on
    `PATH`.
  - Different operating systems and package managers may ship different codec
    coverage, so M6Q must report decode failures clearly rather than promising
    every `.mp4` codec works everywhere.
- `ffmpeg` remains the system-level media decoder diagnostic. If the selected
  Python backend additionally requires `decord`, M6Q should add it to the
  `local-model` runtime profile and add doctor/runtime checks that distinguish
  "Python decoder package missing" from "system codec/decode failed".

## Request Contract

CLI:

```bash
tentgent video understand /path/to/video.mp4 \
  --model-ref <MODEL_REF> \
  --prompt "What happens in this clip?" \
  [--system-prompt "Answer briefly."] \
  [--output answer.md] \
  [--format text|md|json] \
  [--max-tokens 128] \
  [--temperature 0.0] \
  [--sample-fps 1.0] \
  [--max-frames 32] \
  [--max-frame-edge 768] \
  [--clip-start-seconds 0] \
  [--clip-duration-seconds 30]
```

Rules:

- `VIDEO_PATH` must be a local file path.
- `--prompt` is required and must not be blank.
- `--format` defaults to `text`.
- With `--output`, the output file must not already exist.
- Without `--output`, `text`, `md`, and `json` can print to stdout.
- CLI path input is local-only; daemon HTTP clients must upload bytes.

Daemon job:

```http
POST /v1/video/understanding/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `video` | yes | file bytes | Exactly one video file part. |
| `model_ref` | yes | text | Local `video-understanding` model ref or unique alias. |
| `prompt` | yes | text | User question or instruction for the video. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `md`, or `json`; defaults to `text`. |
| `output_filename` | no | text | File name only, not a path. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional generation temperature. |
| `sample_fps` | no | float text | Optional decode/sample rate. |
| `max_frames` | no | integer text | Optional sampled-frame cap. |
| `max_frame_edge` | no | integer text | Optional resize bound for decoded frames. |
| `clip_start_seconds` | no | float text | Optional start offset. |
| `clip_duration_seconds` | no | float text | Optional clip duration bound. |

M6Q should reject:

- missing or duplicate `video`
- duplicate critical text fields
- unknown multipart fields
- blank `model_ref`
- blank `prompt`
- unsupported `output_format`
- unsafe `output_filename`
- uploaded video file parts above `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`
- sample controls outside allowed bounds

Suggested initial bounds, to be adjusted after fixture smoke:

- `sample_fps`: `0.1..=4.0`, default `1.0`
- `max_frames`: `1..=128`, default `32`
- `max_frame_edge`: `128..=1536`, default `768`
- `clip_start_seconds`: `>= 0`, default unset or `0`
- `clip_duration_seconds`: `> 0`, default unset, but runtime should still
  enforce `max_frames`

Suggested video upload cap:

- `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`, default `512 MiB`
- invalid, empty, or zero values fall back to the default and should be logged
  or surfaced as a warning consistently with the generic media upload cap
- `413 video_upload_too_large` when the uploaded video exceeds the video cap

Result route:

```http
GET /v1/video/understanding/job/{job_id}/result?cursor=0&max_chunks=32
```

Result semantics should mirror audio transcription and audio speech:

- `409 result_pending` before result chunks are available.
- `409 job_failed`, `job_interrupted`, or `job_canceled` for terminal jobs
  without an artifact.
- `404 result_not_found` for successful jobs with no result file.
- `Content-Type: text/plain`, `text/markdown`, or `application/json` depending
  on output format.
- `Content-Disposition` with the output file name.
- Cursor headers for chunked result reads.

`json` output should be an envelope, not schema-constrained model output:

```json
{
  "model_ref": "<resolved-model-ref>",
  "output_format": "json",
  "text": "The clip shows ...",
  "finish_reason": "stop",
  "video": {
    "sample_fps": 1.0,
    "max_frames": 32,
    "sampled_frames": 16
  }
}
```

## Kernel Plan

Add the new capability to model metadata:

- `ModelCapability::VideoUnderstanding`
- parse/display string: `video-understanding`
- update model capability docs, CLI parser expectations, API capability lists,
  and model fixture docs
- do not infer this capability from Hugging Face metadata in M6Q unless the
  metadata signal is unambiguous; require explicit `--capability
  video-understanding` for smoke tests

Add a new feature area:

- `src/tentgent-kernel/src/features/video_understanding/`

Domain additions:

- `VideoUnderstandingOutputFormat`
  - `Text`
  - `Markdown`
  - `Json`
- `VideoUnderstandingBackend`
  - `TransformersVideoUnderstanding`
  - `MlxVlm` only if a stable MLX video path is verified; otherwise return a
    planned-backend error
- `VideoUnderstandingRuntimeTarget`
- `ResolvedVideoUnderstandingTarget`
- `VideoSamplingOptions`
- `VideoUnderstandingRequest`
- `VideoUnderstandingResponse`

Use cases:

- `VideoUnderstandingPreparationRequest`
- `VideoUnderstandingPreparationResult`
- `VideoUnderstandingExecutionResult`
- `VideoUnderstandingPreparationUseCase`
- `VideoUnderstandingUseCase`

Ports and infra:

- Add a model resolver requiring `video-understanding`.
- Route safetensors models to the Transformers video backend.
- Do not route normal `vision-chat` models to video by default.
- Add a runtime port implementation that calls a Python one-shot command such
  as `tentgent-video-understanding`.

## Python Runtime Plan

Add a runtime module:

- `python/tentgent-daemon/src/tentgent_daemon/runtime/video_understanding.py`

Add a one-shot CLI:

- `python/tentgent-daemon/src/tentgent_daemon/cli/video_understanding.py`
- package script: `tentgent-video-understanding`

Backend work:

- Add `VideoUnderstandingBackend` to `backends/base.py`.
- Add `create_video_understanding_backend` to `backends/__init__.py`.
- Add a Transformers video backend that prefers the model processor's native
  video path when it supports `{"type": "video", "path": ...}`.
- If the selected model cannot consume video paths directly but can consume
  interleaved images, add a bounded frame-sampling adapter only inside this
  runtime. Do not add generic dynamic transduction in M6Q.
- Add `decord` to the `local-model` optional dependency only if the verified
  fixture needs it; the SmolVLM2 model card names `decord` for video inference.
- Keep MLX video understanding as a planned-backend error unless a small
  Apple Silicon fixture and stable `mlx-vlm` video call path are verified.

Runtime rules:

- Validate model capability before loading.
- Validate video path existence and size.
- Normalize output format.
- Validate prompt and optional system prompt.
- Normalize sampling options.
- Produce text, Markdown, or JSON output files.
- Fail clearly when video decode dependencies are missing or when the installed
  decoder cannot read the selected container/codec combination.

## Daemon Plan

Public routes:

- `POST /v1/video/understanding/job`
- `GET /v1/video/understanding/job/{job_id}/result`

Implementation placement:

- Add `src/tentgent-daemon/src/handlers/rest/video/`.
- Keep route-specific multipart parsing in the daemon handler.
- Add video-specific upload cap handling:
  - read `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`
  - default to `512 MiB`
  - return `413 video_upload_too_large` when exceeded
  - do not change image/audio upload defaults
- Store uploaded video bytes in the job workspace input stream before worker
  execution.
- Use `JobKind::video_understanding`.
- Label jobs as `understand video`.
- Target section should be `video`, reference should be the model ref.
- Store the text-like result in the job workspace result stream.
- Declare one result file with the chosen output filename.

Avoid:

- public spool/chunk routes
- path-input JSON routes
- storing the input video as a managed model/dataset/media object
- deleting the workspace before normal job cleanup retention

## CLI Plan

Add a foreground command:

- `src/tentgent-cli/src/cli/commands/video.rs`
- `src/tentgent-cli/src/cli/video.rs`

Suggested shape:

```text
tentgent video understand <VIDEO_PATH>
  --model-ref <MODEL_REF>
  --prompt <TEXT>
  [--system-prompt <TEXT>]
  [--output <PATH>]
  [--format text|md|json]
  [--max-tokens <N>]
  [--temperature <FLOAT>]
  [--sample-fps <FLOAT>]
  [--max-frames <N>]
  [--max-frame-edge <N>]
  [--clip-start-seconds <FLOAT>]
  [--clip-duration-seconds <FLOAT>]
```

Behavior:

- Read the video from the local path.
- Reject missing video and blank prompt before runtime work starts.
- Reject existing output path.
- Execute the kernel use case directly, not through daemon.
- Print text-like formats to stdout when no output path is supplied.
- Print a short completion message when writing to a file.

## Documentation Plan

Update:

- `README.md`
- `docs/contracts/model-store.md`
- `docs/contracts/job-workspace.md`
- `docs/contracts/platform-backends.md`
- `docs/user/README.md`
- `docs/user/api.md`
- `docs/user/commands.md`
- `docs/user/model-fixtures.md`
- `docs/user/runtime.md`
- `docs/user/version.md`
- `docs/plans/archive/capability-first-release-roadmap.md`

Document:

- `video-understanding` capability.
- `tentgent video understand`.
- Daemon `POST /v1/video/understanding/job`.
- Daemon result route.
- File-byte upload semantics.
- Dedicated video upload cap:
  `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`; keep `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`
  for image/audio multipart routes.
- Sampling controls and defaults.
- Runtime dependency expectations: `local-model`, `ffmpeg`, and possibly
  `decord`.
- Decoder ownership: Tentgent bootstraps Python packages where possible, while
  OS/native codecs are user or operator environment requirements.
- The boundary that M6Q is batch video understanding, not live streaming and
  not video generation.

## Fixture And Research Notes

Primary fixture candidate:

- [`HuggingFaceTB/SmolVLM2-256M-Video-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM2-256M-Video-Instruct)

Current model-card facts to verify during implementation:

- Public, Apache-2.0, safetensors, about 0.3B parameters.
- Designed for image, multi-image, video, and text to text.
- The model card shows Transformers video inference using content item
  `{"type": "video", "path": "path_to_video.mp4"}`.
- The model card says video inference needs `decord`.
- The model card states the 256M video model requires about 1.38 GB of GPU RAM
  for video inference.

Smoke video input:

- Prefer generating a tiny temporary MP4 under `/private/tmp` from existing
  test assets during smoke tests.
- Do not commit binary video fixtures unless the fixture is tiny, durable, and
  explicitly approved.

## Test Plan

Rust unit tests:

- `ModelCapability::VideoUnderstanding` parse/display/error message.
- Video output format parse/display/default filename/media type.
- Sampling option validation and defaulting.
- Model resolver accepts explicit `video-understanding` safetensors models.
- Resolver rejects `vision-chat`, `image-generation`, and other capabilities.
- Runtime command arguments include video path, prompt, format, sampling
  options, and output path.
- CLI parse test for `tentgent video understand`.
- CLI output preflight rejects existing output files.

Daemon tests:

- `POST /v1/video/understanding/job` accepts valid multipart upload and returns
  `202`.
- Missing `video` returns `400`.
- Duplicate `video` or critical text fields return `400`.
- Unsupported media type returns `415` or `400` with a stable error code.
- Invalid sample controls return `400`.
- Result before completion returns `409 result_pending`.
- Result download returns the expected text media type and cursor headers.

Python tests:

- Runtime plan validates `video-understanding` capability.
- Prompt and output format validation.
- Sampling option validation.
- Transformers backend builds model messages with video path.
- Missing decode dependency error is clear.
- JSON output envelope is stable.
- MLX planned-backend error, if no stable MLX video path is implemented.

Smoke:

- Pull `HuggingFaceTB/SmolVLM2-256M-Video-Instruct` with
  `--capability video-understanding`.
- Run `tentgent video understand` against a tiny MP4 and verify text output.
- Run daemon `POST /v1/video/understanding/job`, poll `/v1/jobs/{job_id}`,
  download result, and verify content type and non-empty text.
- Record whether the fixture is suitable for Apple Silicon Pro-class machines
  or should remain developer-only.

## Execution Steps

1. Add `video-understanding` capability metadata and docs.
2. Add kernel video-understanding domain, resolver, use-case, and runtime port.
3. Add Python video-understanding runtime module and one-shot command.
4. Add or gate `decord` in the local-model runtime profile after verifying
   fixture needs.
5. Implement the Transformers video backend using bounded video path intake or
   bounded frame-sampling fallback.
6. Add foreground `tentgent video understand`.
7. Add daemon multipart job route and workflow-owned result route.
8. Add kernel, CLI, daemon, and Python tests.
9. Run fixture smoke with SmolVLM2-256M-Video-Instruct or keep M6Q as
   contract-only if runtime/dependency constraints are not acceptable.
10. Update user/API/runtime/model-fixture/version docs and roadmap status.

## Acceptance Criteria

- `video-understanding` is an accepted model capability.
- `tentgent video understand` can run one local video and prompt through the
  kernel use case.
- `POST /v1/video/understanding/job` creates a daemon job from uploaded video
  bytes.
- `GET /v1/video/understanding/job/{job_id}/result` returns the generated
  text-like result through workflow-owned result semantics.
- CLI and daemon reject unsupported formats, unsafe output filenames, empty
  prompts, invalid sampling controls, oversized video uploads, and
  incompatible model capabilities.
- Runtime decode/sampling is bounded and does not load whole videos into
  memory.
- A small fixture is either smoke-tested or the plan explicitly records why
  M6Q remains contract-only.
