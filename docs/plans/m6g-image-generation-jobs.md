# M6G Image Generation Jobs

Status: implemented and CLI smoke-tested.

M6G's scope is the native text-to-image workflow only. Advanced image workflows
have moved to later standalone roadmap slices so this plan can remain a
completed implementation record.

Depends on:

- [M6A multimodal contracts](./m6a-multimodal-contracts.md)
- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6F vision chat image input](./m6f-vision-chat-image-input.md)

## Goal

Make `image-generation` runnable as the first native image artifact workflow:

```bash
tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --prompt "A small watercolor cabin at sunrise" \
  --output image.png \
  --width 512 \
  --height 512 \
  --steps 20 \
  --seed 7
```

HTTP integrations should use a daemon job route because image generation is
slower than text/image understanding and produces durable binary artifacts:

```http
POST /v1/images/generations/job
Content-Type: application/json
```

M6G is text-to-image only. It should prove the kernel domain, Diffusers runtime,
daemon job artifact routes, foreground CLI, and user-facing file retrieval
contract before adding image-to-image, masks, inpainting, multiple outputs,
compatible OpenAI image APIs, or direct server routes.

## Product Decisions

- `image-generation` remains separate from `vision-chat` and text-only `chat`.
- The first M6G input is text prompt plus bounded generation settings.
- The first M6G output is one generated image artifact.
- Daemon generation is job-based. Users create a job, inspect it with the
  existing `/v1/jobs/{job_id}` route, and fetch generated files through
  image-generation result routes.
- Foreground CLI calls kernel use cases directly and writes a caller-local file.
  It does not require or create daemon jobs.
- CLI `--output` is required for image generation because generated binary image
  bytes should not be printed to the terminal.
- CLI must fail before running when the output file already exists.
- Daemon requests do not accept a caller filesystem output path. The daemon owns
  job workspaces and result files.
- First slice generates exactly one image. File-list routes are still used so
  later multi-image output can extend the contract without replacing endpoints.
- `png` is the default output format. `jpg`/`jpeg` can be supported if the
  runtime can encode it cleanly.
- Image LoRA, image-to-image, masks/inpainting, reference images, and ControlNet
  are later standalone roadmap slices, not part of M6G.
- Apple Silicon acceleration parity is handled by the later MLX media backend
  work. M6G's completed runtime path is Diffusers, with CPU/MPS fp32 and CUDA
  fp16 dtype defaults for stability.
- No prompt template system in M6G.
- No OpenAI-compatible `POST /v1/images/generations` route in M6G. That belongs
  after the native artifact contract is stable.
- No `tentgent server` route in M6G. The roadmap's media serving decision slice
  decides direct media serving.

## Native Request Contract

### Daemon HTTP

Create a generation job:

```http
POST /v1/images/generations/job
Content-Type: application/json
```

Request body:

```json
{
  "model_ref": "<image-generation-model-ref>",
  "prompt": "A small watercolor cabin at sunrise",
  "negative_prompt": "blur, low quality",
  "output_format": "png",
  "output_filename": "cabin.png",
  "width": 512,
  "height": 512,
  "steps": 20,
  "guidance_scale": 7.5,
  "seed": 7
}
```

Fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `model_ref` | yes | string | Local `image-generation` model ref or unique alias. |
| `prompt` | yes | string | Text prompt. Blank prompts are rejected. |
| `negative_prompt` | no | string | Optional negative prompt. Blank values are treated as absent. |
| `output_format` | no | string | `png`, `jpg`, or `jpeg`; defaults to `png`. |
| `output_filename` | no | string | File name only, not a path. Defaults from format. |
| `width` | no | integer | Pixel width. Defaults to `512`; must be bounded and divisible by 8. |
| `height` | no | integer | Pixel height. Defaults to `512`; must be bounded and divisible by 8. |
| `steps` | no | integer | Diffusion step count. Defaults to a conservative value such as `20`. |
| `guidance_scale` | no | number | Optional classifier-free guidance scale. |
| `seed` | no | integer | Optional deterministic seed. When absent, runtime may choose one and report it. |

Initial validation bounds:

- `prompt`: non-empty after trimming, metadata-sized, for example at most
  8192 bytes.
- `negative_prompt`: metadata-sized, for example at most 8192 bytes.
- `width` and `height`: 64 to 1024 inclusive, divisible by 8.
- `steps`: 1 to 100 inclusive.
- `guidance_scale`: 0.0 to 30.0 inclusive.
- `output_filename`: file name only, sanitized or rejected when it contains path
  separators.

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "image_generation",
    "status": "queued",
    "target": {
      "section": "image",
      "reference": "<resolved-model-ref>"
    }
  }
}
```

Inspect status:

```http
GET /v1/jobs/{job_id}
```

List generated files:

```http
GET /v1/images/generations/job/{job_id}/files
```

Pending or failed behavior should mirror audio result routes:

| State | HTTP | Error code |
| --- | --- | --- |
| Job queued/running/intake | `409` | `result_pending` |
| Job failed | `409` | `job_failed` |
| Job interrupted | `409` | `job_interrupted` |
| Job canceled | `409` | `job_canceled` |
| Job succeeded but artifact missing | `404` | `result_not_found` |
| Result ready | `200` | JSON file list |

Ready file-list response:

```json
{
  "job_id": "job-...",
  "files": [
    {
      "file_id": "image-0",
      "filename": "cabin.png",
      "media_type": "image/png",
      "bytes": 12345,
      "width": 512,
      "height": 512,
      "seed": 7
    }
  ]
}
```

Fetch one generated file:

```http
GET /v1/images/generations/job/{job_id}/files/{file_id}
```

The file route returns bytes with:

- `Content-Type`: `image/png` or `image/jpeg`
- `Content-Disposition`: attachment with the generated filename
- `x-tentgent-job-id`
- `x-tentgent-file-id`

Deleting generated artifacts remains the existing job deletion flow:

```http
DELETE /v1/jobs/{job_id}
```

### CLI

Command group:

```text
tentgent image generate
```

Required options:

- `--model-ref <MODEL_REF>`
- `--prompt <TEXT>`
- `--output <PATH>`

Optional options:

- `--negative-prompt <TEXT>`
- `--format png|jpg`
- `--width <PX>`
- `--height <PX>`
- `--steps <N>`
- `--guidance-scale <NUMBER>`
- `--seed <N>`

CLI behavior:

- Resolve and run through kernel use cases directly.
- Require an output path.
- Reject an existing output path before runtime work.
- Create parent directories only if an established CLI pattern already does so;
  otherwise require the parent to exist.
- Print a short completion line such as:

```text
image written: /path/to/image.png
```

## Kernel Design

Add a dedicated feature package:

```text
src/tentgent-kernel/src/features/image_generation/
  domain.rs
  ports.rs
  infra/mod.rs
  infra/resolver.rs
  infra/runtime.rs
  usecases/mod.rs
  usecases/port.rs
  usecases/generation.rs
  tests.rs
```

Core domain surface:

- output format, dimensions, prompt/options, runtime target, request, generated
  file metadata, and response types
- preparation/execution use-case ports following the audio and vision patterns
- model resolver and runtime client ports

Resolver behavior:

- Require `ModelCapability::ImageGeneration`.
- Reject chat, embedding, rerank, audio, and vision-chat models before runtime.
- Prefer a Diffusers-compatible local model layout.
- Return a clear `ImageGenerationRuntimeUnavailable` or model compatibility
  error when the stored model cannot be used for image generation.

## Model Format And Backend Boundary

M6G should add a Diffusers-aware model format/backend boundary rather than
overloading `transformers-peft`.

Model-store changes should add a Diffusers-aware boundary if the current
`safetensors` format marker is not precise enough. The likely implementation is
`ModelFormat::Diffusers`, detected by `model_index.json`, with existing
`safetensors`, `gguf`, and `mlx` behavior preserved for other endpoint
families. Update model-store contracts if a new persisted format is introduced.

Python backend changes:

```text
python/tentgent-daemon/src/tentgent_daemon/runtime/image_generation.py
python/tentgent-daemon/src/tentgent_daemon/cli/image_generate_once.py
python/tentgent-daemon/src/tentgent_daemon/backends/diffusers.py
python/tentgent-daemon/tests/test_image_generation.py
```

Add a new script:

```text
tentgent-image-generate-once
```

Add `diffusers` to the `local-model` profile, plus `accelerate` if the selected
pipeline path requires it. Doctor/runtime checks should report Diffusers
readiness. The runtime should save the generated image to the output path
selected by Rust and return JSON metadata; image bytes should never go through
stdout.

## Daemon Design

Add a REST handler package:

```text
src/tentgent-daemon/src/handlers/rest/images/mod.rs
```

Route registration:

```text
POST /v1/images/generations/job
GET  /v1/images/generations/job/{job_id}/files
GET  /v1/images/generations/job/{job_id}/files/{file_id}
```

Daemon worker flow: validate request, reject incompatible models early, create
`JobKind::image_generation()`, open a job workspace, write output under
`files/`, run the kernel use case in a blocking worker, register the result
file, and mark the job succeeded or failed with a clear summary.

Job behavior:

- Use existing `/v1/jobs` routes for list, inspect, cancel, and delete.
- Do not expose workspace or chunk routes.
- Do not accept user-controlled daemon-host output paths.
- Do not promise hard cancellation of an in-flight Python generation process
  unless the runtime client can actually interrupt it safely. A pre-runtime
  cancel check is acceptable; otherwise mark running jobs as not cancellable.

## CLI Design

Add files:

```text
src/tentgent-cli/src/cli/commands/image.rs
src/tentgent-cli/src/cli/image.rs
```

Wire them into:

```text
src/tentgent-cli/src/cli/commands/mod.rs
src/tentgent-cli/src/cli/mod.rs
```

CLI tests should cover command parsing, required arguments, output path exists
rejection, output format aliases, and option validation.

## Documentation Updates

Update:

- `README.md`
- `docs/user/README.md`
- `docs/user/commands.md`
- `docs/user/api.md`
- `docs/user/model-fixtures.md`
- `docs/user/runtime.md`
- `docs/user/version.md`
- `docs/plans/README.md`
- `docs/plans/capability-first-release-roadmap.md`

Docs should say:

- Image generation is text-to-image only in M6G.
- CLI writes a local output file and does not use the daemon.
- Daemon route creates a job and result files are fetched through
  image-generation routes.
- `image-generation` models require Diffusers/local-model dependencies.
- `hf-internal-testing/tiny-stable-diffusion-pipe` is a plumbing smoke fixture,
  not a quality model.
- `segmind/tiny-sd` is a tiny candidate but may be heavier than the internal
  fixture.
- OpenAI-compatible image generation and server routes remain out of scope.

## Tests

Rust kernel:

```bash
cargo test -p tentgent-kernel image_generation
```

Cover output format parsing, option validation, resolver accept/reject behavior,
and runtime client arguments.

Rust daemon:

```bash
cargo test -p tentgent-daemon image_generation
```

Cover request validation, early capability rejection, pending/failed/ready file
route behavior, file metadata, binary response headers, and missing file IDs.

Rust CLI:

```bash
cargo test -p tentgent-cli image
```

Python:

```bash
uv run python -m unittest discover -s tests
```

Cover runtime request parsing, output path handling, Diffusers routing, and
entrypoint JSON response shape.

General checks:

```bash
cargo fmt --check
git diff --check
cargo check --workspace
cargo test --workspace
uv run python -m unittest discover -s tests
```

## Real Smoke Plan

After implementation, test with a tiny Diffusers model:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull hf-internal-testing/tiny-stable-diffusion-pipe \
  --capability image-generation
```

CLI smoke:

```bash
tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --prompt "A tiny red square" \
  --output /private/tmp/tentgent-image-generation-smoke.png \
  --width 64 \
  --height 64 \
  --steps 2 \
  --seed 1
```

Observed implementation smoke:

- `diffusers/tiny-stable-diffusion-torch` generated a 64x64 PNG through the
  foreground CLI.
- `segmind/tiny-sd` generated a 512x512 PNG through the foreground CLI after
  the Diffusers loader forced fp32 on CPU/MPS-class devices.
- Daemon image-generation behavior is covered by route tests; a real daemon
  curl smoke should still be run before a tagged release.

Daemon smoke:

```bash
curl -sS http://127.0.0.1:8790/v1/images/generations/job \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref":"<image-generation-model-ref>",
    "prompt":"A tiny red square",
    "output_format":"png",
    "width":64,
    "height":64,
    "steps":2,
    "seed":1
  }'
```

Then inspect and fetch:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS http://127.0.0.1:8790/v1/images/generations/job/<job-id>/files
curl -sS http://127.0.0.1:8790/v1/images/generations/job/<job-id>/files/image.png \
  -o /private/tmp/tentgent-image-generation-daemon.png
```

## Acceptance Criteria

- `image-generation` models can be resolved and rejected correctly by kernel
  capability gates.
- A foreground CLI command can generate one local image file and refuses to
  overwrite existing output.
- Daemon `POST /v1/images/generations/job` creates a job and produces one image
  artifact in the job workspace.
- Result file list and file download routes work without exposing workspace or
  spool internals.
- Runtime dependency readiness includes Diffusers in the local-model profile.
- User docs explain CLI usage, daemon job usage, output file behavior, model
  fixture expectations, and current non-goals.

## Moved To Later Roadmap Slices

The following image workflows are not part of M6G. They are tracked as later
standalone roadmap slices after the MLX image backend decision:

- Image generation LoRA.
- Image-to-image.
- Inpainting and masks.
- Reference images and ControlNet.

## Deferred Beyond M6G

- OpenAI-compatible `/v1/images/generations`.
- Direct `tentgent server` image-generation routes.
- Multiple generated images per request.
- Streaming partial image previews.
- Video generation.
