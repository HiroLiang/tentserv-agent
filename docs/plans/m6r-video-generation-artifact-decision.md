# M6R Video Generation Artifact Decision

Status: implemented as an internal contract slice. Do not expose a public
video-generation capability, CLI command, or daemon route unless a practical
local fixture is approved first.

M6R decides whether local video generation belongs in the current M6-to-M7
release line. It is intentionally separate from M6Q: video understanding consumes
bounded video input and returns text, while video generation produces large
playable media artifacts and needs encoder, temporary disk, model-runtime, and
hardware limits before it can be a user-facing workflow.

## Decision Summary

M6R defines the future artifact contract and execution gate, but it does not
expose a public video-generation workflow by default.

Reason: there is no small, representative, product-quality Hugging Face fixture
that is safe to treat as the default local smoke model for this release line.
Tiny dummy models exist, but they are suitable only for internal plumbing tests,
not for proving that users can run a useful video-generation workflow.

## Depends On

- [M6B kernel job workspace foundation](./m6b-kernel-job-workspace-foundation.md)
- [M6G image generation jobs](./m6g-image-generation-jobs.md)
- [M6K MLX image generation backend decision](./m6k-mlx-image-generation-backend-decision.md)
- [M6Q video understanding jobs](./m6q-video-understanding-jobs.md)

## Fixture Research

| Candidate | Task | Decision |
| --- | --- | --- |
| [`Jingya/tiny-stable-video-diffusion-img2vid`](https://huggingface.co/Jingya/tiny-stable-video-diffusion-img2vid) | Image-to-video dummy Diffusers fixture | Internal plumbing only. The model card says it is a random dummy model for internal testing and should not be used in other scenarios. |
| [`Wan-AI/Wan2.1-T2V-1.3B-Diffusers`](https://huggingface.co/Wan-AI/Wan2.1-T2V-1.3B-Diffusers) | Real text-to-video Diffusers model | Plausible future candidate, but not a small smoke fixture. The model card targets 480P output and documents notable GPU memory/time expectations. |
| [`camenduru/potat1`](https://huggingface.co/camenduru/potat1) | Diffusers text-to-video prototype | Not a release fixture. The model card describes it as a prototype trained on a small dataset and based on the older ModelScope text-to-video line. |
| [`stabilityai/stable-video-diffusion-img2vid`](https://huggingface.co/stabilityai/stable-video-diffusion-img2vid) | Image-to-video | Not a small smoke fixture and has license/commercial-use considerations. Useful later for an image-to-video contract discussion. |

M6R conclusion: keep `video-generation` out of accepted public capability
values until a real fixture can be verified on target local hardware, or until a
separate post-M7 compatibility plan owns heavyweight generation support.

## Implemented Shape

M6R adds only the kernel-internal video-generation artifact domain:

- `src/tentgent-kernel/src/features/video_generation/`
- `VideoGenerationOutputFormat` for `mp4` and `webm`
- `VideoGenerationPrompt`
- `VideoGenerationDimensions`
- `VideoGenerationOptions`
- `VideoGenerationInput`
- `VideoGenerationArtifactPlan`
- `VideoGenerationArtifact`

The implementation intentionally does not add:

- `ModelCapability::VideoGeneration`
- `tentgent model pull --capability video-generation`
- `tentgent video generate`
- daemon video-generation routes
- Python runtime entrypoints
- user-facing fixture documentation

## Scope

In scope:

- Decide whether to expose video generation in the current release line.
- Record the minimum public contract required before implementation.
- Define a fixture gate for any future implementation.
- Define artifact shape, output files, retention, and cleanup expectations.
- Define encoder and temporary disk dependency boundaries.
- Keep implementation dormant if no fixture passes.

Out of scope:

- Public `video-generation` capability acceptance.
- Public CLI command.
- Public daemon route.
- Realtime video generation streaming.
- Live camera input.
- Video editing or video-to-video.
- Audio generation, soundtrack generation, or audio/video sync.
- OpenAI, Gemini, or other hosted video API compatibility.
- `tentgent server` video-generation routes.
- MLX video-generation support before a concrete local stack is proven.

## Fixture Gate

Do not start public implementation until one approved fixture satisfies all of
these:

- The model is publicly accessible and can be pulled through the managed model
  store without manual file patching.
- The model has a clear local runtime path, such as Diffusers, with a stable
  Python API.
- The model can produce a playable `mp4` or `webm` on the target developer
  machine using bounded settings.
- The smoke test completes without exhausting memory or temporary disk.
- The result is representative enough to show the real workflow, not only a
  random dummy pipeline.
- License and gating requirements are documented in user-facing model fixtures
  before exposing commands.

If only a dummy model is available, implementation may be limited to internal
unit tests for request validation and artifact manifests, but it must not be
documented as a user-facing capability.

## Future Public Surface

These names are reserved only if the fixture gate passes.

CLI:

```bash
tentgent video generate \
  --model-ref <video-generation-model-ref> \
  --prompt "A small paper boat crossing a rain puddle." \
  --output out.mp4 \
  --duration-seconds 2 \
  --fps 8 \
  --width 256 \
  --height 256
```

Daemon job:

```http
POST /v1/video/generations/job
Content-Type: multipart/form-data
```

Result routes:

```http
GET /v1/video/generations/job/{job_id}/files
GET /v1/video/generations/job/{job_id}/files/{file_id}
```

These should remain workflow-owned routes, not generic spool APIs.

## Request Contract

Text-to-video should be the first candidate contract if a fixture is approved:

| Field | Required | Notes |
| --- | --- | --- |
| `model_ref` | yes | Local model ref with verified `video-generation` support. |
| `prompt` | yes | Non-empty generation prompt. |
| `negative_prompt` | no | Runtime-dependent; pass only when supported. |
| `duration_seconds` | no | Early cap should be small, for example 2 to 4 seconds. |
| `fps` | no | Early cap should be low, for example 8 to 12 FPS. |
| `width` | no | Bounded by runtime support and hardware limits. |
| `height` | no | Bounded by runtime support and hardware limits. |
| `num_frames` | no | Alternative to duration and FPS for frame-count-first runtimes. |
| `steps` | no | Runtime-dependent diffusion step count. |
| `guidance_scale` | no | Runtime-dependent guidance control. |
| `seed` | no | Optional deterministic generation seed. |
| `output_format` | no | `mp4` first if encoder support is verified; `webm` later. |
| `output_filename` | no | File name only, not a path. |

Image-to-video should be a later extension because it changes the request from
text-only to file-plus-text and should inherit lessons from M6M image-to-image.

## Artifact Contract

- Video generation must be job-only.
- The primary result must be one playable media file, preferably `mp4` once
  encoder support is verified.
- `webm` can be added only after encoder availability and browser/player
  compatibility are documented.
- Raw frames may be retained only as debug or advanced artifacts.
- If raw frames are ever exposed, they must be listed separately from the
  primary playable result and cleaned by the same job retention rules.
- Result downloads must stream from disk instead of loading the whole video into
  memory.
- Job status must report `queued`, `running`, `succeeded`, `failed`,
  `interrupted`, or `canceled` consistently with existing media jobs.

## Runtime And Dependency Boundary

- The first runtime candidate should be Diffusers only if the approved fixture
  has a stable Diffusers pipeline.
- The Python runtime should receive bounded plan data and write files to the job
  workspace. It should not return video bytes through stdout.
- Encoder ownership must be explicit:
  - Python package dependencies belong in the `local-model` runtime profile.
  - System encoders such as `ffmpeg` remain operator/system dependencies unless
    a packaged encoder is approved later.
- `tentgent doctor` should eventually report encoder availability and install
  hints before public exposure.
- MLX video generation is deferred until an MLX-compatible video-generation
  stack and fixture are identified.

## Limits And Cleanup

Before public exposure, add video-generation-specific limits instead of reusing
generic image or video-understanding limits:

- `TENTGENT_VIDEO_GENERATION_MAX_OUTPUT_BYTES`
- `TENTGENT_VIDEO_GENERATION_TEMP_MAX_BYTES`
- Optional runtime timeout, such as
  `TENTGENT_VIDEO_GENERATION_RUNTIME_TIMEOUT_SECONDS`

The job workspace must keep normal retention buffer behavior:

- Do not delete a completed result immediately after success.
- Preserve failed job logs long enough for diagnosis.
- Let `daemon stop` cancel running jobs and trigger one cleanup pass.
- Let explicit job deletion remove generated artifacts after the retention
  buffer policy allows it.

## Execution Plan

1. Create this M6R decision plan and link it from the active roadmap.
2. Record the model fixture conclusion: no suitable small real fixture is
   approved for public smoke testing.
3. Add the kernel-internal video-generation artifact domain and validation
   tests.
4. Keep `video-generation` out of the public capability vocabulary and user
   documentation for now.
5. Do not add `tentgent video generate` or daemon generation routes in M6R.
6. If a real fixture is later approved, open a separate implementation slice
   that adds capability acceptance, CLI, daemon route, Python runtime wiring,
   doctor checks, docs, and real-model smoke evidence together.

## Acceptance Criteria

- M6R has a standalone plan document.
- The active roadmap links to the plan and states the no-public-exposure gate.
- The plan records the currently known model candidates and why they are not a
  sufficient small smoke fixture.
- Kernel domain tests cover output formats, prompt trimming, dimensions, and
  small-frame limits.
- No public code path is added unless a fixture gate passes in a later approved
  implementation slice.
