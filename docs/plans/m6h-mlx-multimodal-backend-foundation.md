# M6H MLX Multimodal Backend Foundation

Status: planned.

Depends on:

- [M6A multimodal contracts](./m6a-multimodal-contracts.md)
- [M6F vision chat image input](./m6f-vision-chat-image-input.md)
- [M6G image generation jobs](./m6g-image-generation-jobs.md)

## Goal

Make Apple Silicon local deployment a first-class backend target for media
workflows. M6C through M6G prove the native audio transcription, vision chat,
and image generation surfaces. M6H adds the missing backend-family foundation so
those same surfaces can route to MLX runtimes when practical.

The product goal is not "MLX chat only." A user with a Mac Studio, Mac mini, or
Apple Silicon laptop should be able to deploy useful local media tools without
being forced onto CPU-only PyTorch paths whenever a compatible MLX runtime
exists.

## Product Decision

Insert MLX media backend work before opening additional media surfaces such as
audio speech and video. The current safetensors/Diffusers baselines remain
implemented and usable. M6H should not rewrite those surfaces; it should make
backend selection broader and more explicit.

## Runtime Families

Do not route every `ModelFormat::Mlx` model to `mlx-lm`.

Planned MLX family split:

| Family | Tentgent workflows | Candidate runtime | Notes |
| --- | --- | --- | --- |
| `mlx-lm` | text `chat`, MLX LoRA | `mlx-lm` | Already used for chat and training. |
| `mlx-vlm` | `vision-chat` | `mlx-vlm` | Image plus text to text; should reuse native vision APIs. |
| `mlx-audio` | `audio-transcription`, future `audio-speech` candidates | `mlx-audio` | Whisper-style ASR and some TTS/audio-understanding candidates. |
| `mlx-diffusion` | `image-generation` | DiffusionKit or approved MLX diffusion runtime | Decision slice before advanced image generation work. |

## Kernel Shape

- Keep `ModelFormat::Mlx` as the persisted storage format unless a stronger
  split is needed.
- Add a kernel-owned MLX runtime-family concept that is separate from storage
  format and serving capability.
- Let feature resolvers choose a backend from both serving capability and MLX
  family:
  - `chat` + `mlx-lm` -> current MLX chat backend
  - `vision-chat` + `mlx-vlm` -> future MLX VLM backend
  - `audio-transcription` + `mlx-audio` -> future MLX ASR backend
  - `image-generation` + `mlx-diffusion` -> future MLX image backend
- Preserve explicit user capability overrides and existing model refs.
- Reject mismatched MLX families early with clear errors instead of attempting
  to load them through the wrong Python package.

## Python Runtime Shape

Add runtime adapters only after the package and smoke model are approved:

```text
python/tentgent-daemon/src/tentgent_daemon/backends/mlx_vlm.py
python/tentgent-daemon/src/tentgent_daemon/backends/mlx_audio.py
python/tentgent-daemon/src/tentgent_daemon/backends/mlx_diffusion.py
```

Potential entry points should mirror existing feature-owned entry points instead
of creating generic media runners.

Doctor/runtime readiness should probe packages by family:

- `mlx-lm` for existing chat/training
- `mlx-vlm` for vision chat
- `mlx-audio` for audio workflows
- a chosen diffusion runtime package only after M6K approves one

## User Surface

No new user API is required for M6H itself. Users should keep using the native
feature commands and endpoints:

- `tentgent vision chat <IMAGE_PATH>`
- `tentgent transcribe <AUDIO_PATH>`
- `tentgent image generate`
- `POST /v1/vision/chat`
- `POST /v1/audio/transcriptions/job`
- `POST /v1/images/generations/job`

Backend choice should be visible in model inspect, job/server details, or CLI
verbose/plan output once the runtime-family selector exists.

## Candidate Model Families

These candidates prove availability, not final support:

- MLX VLM: small `mlx-community` VLM repos such as SmolVLM, LFM2-VL, and
  Qwen2.5-VL variants.
- MLX audio: Whisper ASR variants and Kokoro-style TTS candidates when package
  APIs are stable enough.
- MLX image generation: DiffusionKit or other MLX Stable Diffusion-compatible
  runtimes, evaluated separately because Diffusers pipelines cannot be loaded by
  `mlx-lm`.

## Non-Goals

- Do not replace the implemented safetensors/Diffusers baselines.
- Do not add OpenAI/Claude/Gemini multimodal compatibility.
- Do not add server media routes.
- Do not add video runtime support.
- Do not claim MLX support for a workflow until there is a real smoke model and
  runtime package path.

## Acceptance Criteria

- The roadmap and model routing rules distinguish MLX storage format from MLX
  runtime family.
- Existing MLX chat behavior remains unchanged.
- Future MLX media slices have a clear place to add dependencies, doctor
  checks, backend adapters, resolver logic, and smoke tests.
- The next implementation slice can choose MLX vision chat or MLX audio without
  refactoring the whole model store again.
