# M6F Vision Chat Image Input

Status: implemented.

Depends on:

- [M6A multimodal contracts](./m6a-multimodal-contracts.md)
- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6E audio transcription CLI and large-file hardening](./m6e-audio-transcription-cli-and-large-file-hardening.md)

## Goal

Make `vision-chat` runnable as a first native image-plus-text workflow without
folding image payloads into the existing text-only chat DTOs:

```bash
tentgent vision chat /path/to/image.png \
  --model-ref <vision-chat-model-ref> \
  --prompt "Describe the image." \
  --output answer.md \
  --format md
```

HTTP integrations should use a native multipart endpoint:

```http
POST /v1/vision/chat
Content-Type: multipart/form-data
```

M6F is a bounded single-image workflow. It should prove the domain, runtime, CLI,
and daemon endpoint shape before adding multi-image, job, direct server, or
provider-compatible multimodal routes.

## Product Decisions

- `vision-chat` remains separate from text-only `chat`.
- The first public image input is multipart file bytes or a CLI-local file path.
- One request means one logical image and one prompt.
- Image bytes are transport-stream-friendly at daemon intake, but the model sees
  a complete image file/path after upload completes.
- Foreground CLI calls kernel use cases directly and does not require the daemon.
- Daemon `POST /v1/vision/chat` is synchronous and bounded in M6F.
- No durable job route is implemented in M6F. If real smoke tests show local VLM
  latency is not comfortable for synchronous HTTP, split `POST
  /v1/vision/chat/job` into a follow-up slice.
- Session-aware vision chat is out of scope. M6F requests are stateless.

## Native Request Contract

### Daemon HTTP

Route:

```http
POST /v1/vision/chat
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `image` | yes | file bytes | Exactly one image. The daemon does not trust or receive the client's local path. |
| `model_ref` | yes | text | Local `vision-chat` model ref or unique alias. |
| `prompt` | yes | text | User prompt for the image. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `json`, or `md`; defaults to `text`. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional generation temperature. |

M6F should reject:

- missing or duplicate `image`
- duplicate critical text fields
- unknown multipart fields
- blank `model_ref`
- blank `prompt`
- unsupported `output_format`
- oversized metadata fields
- image uploads above the daemon-wide `TENTGENT_MEDIA_UPLOAD_MAX_BYTES` cap,
  which defaults to 20 MiB

Accepted image media types in M6F:

- `image/png`
- `image/jpeg`
- `image/webp`

The daemon may infer media type from `Content-Type` first and filename second.
Decode-level failures stay in the runtime/backend error path so the selected
processor remains the source of truth.

Response shape:

```json
{
  "model_ref": "<resolved-model-ref>",
  "output_format": "text",
  "text": "A short answer from the model.",
  "finish_reason": "stop"
}
```

`output_format` controls response rendering intent, not guaranteed model
semantics:

- `text`: return plain generated text in the JSON envelope.
- `md`: return generated text intended to be written/read as Markdown.
- `json`: return the same JSON envelope. Do not promise schema-constrained
  model output until a later structured-output contract exists.

### CLI

Suggested command group:

```text
tentgent vision chat <IMAGE_PATH>
  --model-ref <MODEL_REF>
  --prompt <TEXT>
  [--system-prompt <TEXT>]
  [--output <OUTPUT_PATH>]
  [--format text|json|md]
  [--max-tokens <N>]
  [--temperature <FLOAT>]
  [--home <HOME>]
```

Output behavior:

- With `--output`, write only to that file and print a concise completion
  message.
- If the output path already exists, fail before runtime execution.
- Without `--output`, print `text` and `md` bodies to stdout.
- Without `--output` and `--format json`, print the response JSON envelope.
- `--format` defaults to `text`.
- Do not add `--detach`.

## Non-Goals

- Do not reuse `ChatMessage { role, content: String }` for image content.
- Do not add OpenAI, Claude, or Gemini compatible multimodal DTO support.
- Do not add `tentgent server` vision routes.
- Do not implement `POST /v1/vision/chat/job` in the first M6F slice.
- Do not add session transcript storage for images.
- Do not add adapter or LoRA selection for vision models.
- Do not support multiple images in one request yet.
- Do not create a permanent media catalog.
- Do not expose generic workspace, chunk, or temp-file APIs.

## Runtime Scope

First backend target:

- Local safetensors `vision-chat` models through Transformers.
- Smoke candidate: `HuggingFaceTB/SmolVLM-256M-Instruct`.

M6F should add a dedicated Python runtime entrypoint rather than overloading
`tentgent-chat-once`:

```text
tentgent-vision-chat-once
```

The runtime should receive:

- model ref
- input image path
- prompt
- optional system prompt
- output format
- generation options
- runtime home

The Python implementation should use a dedicated vision runtime module and
backend boundary. The likely path is Transformers processor/model support for
image-text-to-text models, but the Rust kernel contract should stay independent
from one exact Transformers class.

If the chosen smoke model cannot run reliably on local Apple Silicon in this
slice, keep the runtime contract and fake-entrypoint tests, then mark real model
smoke as blocked with the observed backend error. Do not fake a passing smoke.

## Data Handling Boundary

Image upload is a complete-image workflow:

- The daemon may stream multipart bytes to a request-scoped temp file to avoid
  holding the whole upload in memory.
- The runtime starts after the image file is complete.
- Byte chunking is not model context. The backend decodes the complete image and
  performs its own resize/crop/tensor preparation.
- Temp files must be removed on success and failure.
- The CLI path is local file input; it does not upload or create a job.

## Execution Plan

### 1. Kernel Vision Domain

Add a focused feature package:

- `src/tentgent-kernel/src/features/vision/domain.rs`
- `src/tentgent-kernel/src/features/vision/ports.rs`
- `src/tentgent-kernel/src/features/vision/usecases/port.rs`
- `src/tentgent-kernel/src/features/vision/usecases/vision_chat.rs`
- `src/tentgent-kernel/src/features/vision/infra/resolver.rs`
- `src/tentgent-kernel/src/features/vision/infra/runtime.rs`
- `src/tentgent-kernel/src/features/vision/mod.rs`

Domain types:

- `VisionChatOutputFormat`
- `VisionChatImageInput`
- `VisionChatPrompt`
- `VisionChatGenerationOptions`
- `VisionChatBackend`
- `VisionChatRuntimeTarget`
- `ResolvedVisionChatTarget`
- `VisionChatRequest`
- `VisionChatResponse`

Use-case boundaries:

- `VisionChatModelResolver`
- `VisionChatRuntimeClient`
- `VisionChatPreparationUseCase`
- `VisionChatUseCase`

Resolver rules:

- require `ModelCapability::VisionChat`
- support safetensors first
- reject `chat`, `embedding`, `rerank`, and audio capabilities before runtime
- reject unsupported formats with `UnsupportedTarget`

### 2. Runtime Entrypoint

Add:

- `RuntimeEntrypoint::VisionChatOnce`
- script name `tentgent-vision-chat-once`
- executable resolver tests
- doctor/runtime entrypoint checks

Python files:

- `python/tentgent-daemon/src/tentgent_daemon/runtime/vision.py`
- `python/tentgent-daemon/src/tentgent_daemon/cli/vision_chat_once.py`
- backend factory support in `backends/base.py` and `backends/__init__.py`
- Transformers implementation in a focused backend module or the existing
  Transformers backend file if reuse is small and clear
- `pyproject.toml` script entry

### 3. CLI Surface

Add command parsing and handler:

- `src/tentgent-cli/src/cli/commands/vision.rs`
- `src/tentgent-cli/src/cli/vision.rs`
- command wiring in `commands/mod.rs` and `cli/mod.rs`

Tests:

- parses `tentgent vision chat image.png --model-ref abc --prompt "..."`
- parses optional output format, output path, system prompt, max tokens,
  temperature, and home
- rejects missing prompt/model
- rejects existing output path
- verifies CLI uses kernel directly and does not create daemon jobs

### 4. Daemon Native Endpoint

Add:

- `src/tentgent-daemon/src/handlers/rest/vision/mod.rs`
- route registration for `POST /v1/vision/chat`
- daemon kernel component wiring for vision model resolver and runtime client

Multipart parser behavior:

- stream the `image` part to a request-scoped temp file
- enforce one logical image
- bound metadata fields and image byte size
- remove temp files after completion or error
- pass only daemon-internal temp path to the kernel/runtime

No job workspace route is exposed in M6F.

### 5. Python Runtime Tests

Add tests for:

- output format normalization
- plan building rejects non-vision models
- request rendering with system prompt and user prompt
- fake backend/runtime output JSON
- image path validation

Do not download real Hugging Face models in unit tests.

### 6. Rust Tests

Kernel:

- output format parsing
- prompt validation
- model resolver accepts `vision-chat` safetensors
- resolver rejects non-vision models
- runtime client builds `tentgent-vision-chat-once` arguments

Daemon:

- multipart endpoint accepts one image
- missing image returns `bad_request`
- duplicate image returns `bad_request`
- unknown fields return `bad_request`
- non-vision model is rejected before runtime
- fake runtime response maps to native response shape

CLI:

- parse tests
- output path validation
- fake runtime entrypoint execution where practical

### 7. Documentation

Update:

- `docs/user/commands.md`
- `docs/user/api.md`
- `docs/user/model-fixtures.md`
- `docs/user/version.md`
- `docs/plans/archive/capability-first-release-roadmap.md`
- this plan's status and completion notes after implementation

Document:

- `vision-chat` is native and separate from text-only `chat`
- CLI is foreground and daemon-free
- daemon endpoint is multipart image bytes, exactly one image
- supported image media types and size limit
- no OpenAI/Claude/Gemini multimodal compatibility yet
- no `vision-chat` server route yet

## Likely Files

Kernel:

- `src/tentgent-kernel/src/features/mod.rs`
- `src/tentgent-kernel/src/features/runtime/domain.rs`
- `src/tentgent-kernel/src/features/runtime/tests.rs`
- `src/tentgent-kernel/src/features/doctor/infra/runtime.rs`
- `src/tentgent-kernel/src/features/vision/*`

Python:

- `python/tentgent-daemon/pyproject.toml`
- `python/tentgent-daemon/src/tentgent_daemon/runtime/vision.py`
- `python/tentgent-daemon/src/tentgent_daemon/cli/vision_chat_once.py`
- `python/tentgent-daemon/src/tentgent_daemon/backends/base.py`
- `python/tentgent-daemon/src/tentgent_daemon/backends/__init__.py`
- `python/tentgent-daemon/src/tentgent_daemon/backends/transformers_peft.py`
- `python/tentgent-daemon/tests/test_vision_chat.py`

Daemon:

- `src/tentgent-daemon/src/kernel/mod.rs`
- `src/tentgent-daemon/src/kernel/model.rs`
- `src/tentgent-daemon/src/kernel/runtime.rs`
- `src/tentgent-daemon/src/handlers/rest/mod.rs`
- `src/tentgent-daemon/src/handlers/rest/vision/mod.rs`
- `src/tentgent-daemon/src/transport/rest/router.rs`
- `src/tentgent-daemon/src/transport/rest/tests.rs`

CLI:

- `src/tentgent-cli/src/cli/commands/mod.rs`
- `src/tentgent-cli/src/cli/commands/vision.rs`
- `src/tentgent-cli/src/cli/mod.rs`
- `src/tentgent-cli/src/cli/vision.rs`

Docs:

- `docs/user/commands.md`
- `docs/user/api.md`
- `docs/user/model-fixtures.md`
- `docs/user/version.md`
- `docs/plans/archive/capability-first-release-roadmap.md`
- `docs/plans/m6f-vision-chat-image-input.md`

## Verification

Required local checks:

```bash
cargo fmt --check
cargo check --workspace
cargo test -p tentgent-kernel vision
cargo test -p tentgent-cli vision
cargo test -p tentgent-daemon vision
cargo test --workspace
uv run python -m unittest discover -s tests
```

Recommended real smoke, when the model is available:

```bash
tentgent model pull HuggingFaceTB/SmolVLM-256M-Instruct --capability vision-chat

tentgent vision chat test-data/test_image.png \
  --model-ref <vision-chat-model-ref> \
  --prompt "Describe this image in one sentence." \
  --output /private/tmp/tentgent-vision-answer.md \
  --format md

tentgent daemon start --host 127.0.0.1 --port 8790

curl -sS http://127.0.0.1:8790/v1/vision/chat \
  -F model_ref=<vision-chat-model-ref> \
  -F prompt='Describe this image in one sentence.' \
  -F output_format=text \
  -F image=@test-data/test_image.png
```

## Review Target

- A user can run one local image-plus-prompt request from the CLI without the
  daemon.
- An HTTP integration can send one image as multipart bytes to
  `POST /v1/vision/chat` and receive a native JSON response.
- Text-only chat contracts remain unchanged.
- OpenAI/Claude/Gemini compatible multimodal routes remain explicitly rejected
  until a later compatibility slice.
- No generic upload, chunk, workspace, or media catalog API leaks into the
  product surface.
