# Common Commands

This document collects user-facing command examples. Short references are accepted anywhere a local `model_ref`, `adapter_ref`, `dataset_ref`, or `server_ref` is requested, as long as the prefix is unique.

Most common options have short aliases, such as `-m` for model/message-like inputs, `-o` for output, `-p` for provider/path/port depending on the subcommand, and `-H` for runtime home. Run `tentgent <command> --help`; every help screen also supports `-h`.

## File And HTTP Media Rules

- Foreground CLI media commands read local paths on the caller's machine. They
  do not create daemon jobs unless the command explicitly says it talks to the
  daemon.
- When a CLI media command accepts `--output`, it writes to that path and fails
  before running if the file already exists. Without `--output`, text-like
  formats print to stdout when the format supports terminal output.
- Daemon media endpoints receive multipart file bytes, not client-local paths.
  `curl -F file=@/path/audio.mp3` and `curl -F image=@/path/image.png` are curl
  syntax for reading local bytes into the request; the same applies to
  `curl -F file=@/path/video.mp4`.
- Audio transcription, video understanding, and image generation daemon routes
  create workflow jobs and expose result bytes or files through result routes.
  Native vision chat daemon uploads are bounded synchronous requests.
- Audio/image multipart uploads share `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`,
  defaulting to 20 MiB. Video uploads use `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`,
  defaulting to 512 MiB. Exceeding either cap returns HTTP `413`.

## Auth

Check all provider keys:

```bash
tentgent auth status
```

Set provider keys:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
tentgent auth gemini set
```

Inspect or remove one provider key:

```bash
tentgent auth openai
tentgent auth openai rm
tentgent auth gemini
tentgent auth gemini rm
```

Inspect or set provider auth source modes:

```bash
tentgent auth mode
tentgent auth mode openai
tentgent auth mode openai auto
tentgent auth mode openai env
tentgent auth mode gemini file --path ~/.config/tentgent/provider.env
tentgent auth mode anthropic none
```

Available modes:

- `auto`: request/prompt, `.env` / process env, process cache, Keychain, then
  none.
- `keychain`: only Tentgent-managed system secret storage.
- `file`: only the explicit env file configured with `--path`.
- `env`: only process environment variables.
- `none`: disable local provider auth resolution.

Use `env` when OpenShell or another launcher injects standard variables such as
`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, or `HF_TOKEN`.

`file` mode reads the same variable names from the configured env file:

```dotenv
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GEMINI_API_KEY=...
```

The daemon exposes read-only auth status. Provider key set/remove stays
local-only through the CLI:

```bash
curl -sS http://127.0.0.1:8790/v1/auth \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth/openai \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Daemon auth status reports local env/keychain presence only. It does not print
secrets and does not call provider validation endpoints.

## Runtime

Inspect the managed Python runtime:

```bash
tentgent runtime status
tentgent runtime status --profile full
tentgent runtime status --project /path/to/python-project --env /path/to/python-env
```

Prepare the managed Python runtime after package-manager installs such as
Homebrew:

```bash
tentgent runtime bootstrap
tentgent doctor
```

Install heavier optional runtime profiles only when needed:

```bash
tentgent runtime bootstrap --profile local-model
tentgent runtime bootstrap --profile training
tentgent runtime bootstrap --profile full
tentgent runtime bootstrap --profile all
```

Inspect the paths that would be used without syncing:

```bash
tentgent runtime bootstrap --print-plan
tentgent runtime bootstrap --profile local-model --dry-run
```

Direct release installers run this bootstrap by default. Use this command when
you install from a package manager, intentionally skipped installer bootstrap,
or need to resync Python dependencies after an upgrade.

Clean up abandoned managed-store staging directories after an interrupted
import or pull:

```bash
tentgent store gc
tentgent store gc --apply
```

`tentgent store gc` is a dry run by default. It only targets direct children of
`models/staging`, `adapters/staging`, and `datasets/staging`; hashed
`store/<ref>` content is not removed.

## Models

Import a local model:

```bash
tentgent model add /path/to/local-model
tentgent model add ./models/bge-small --capability embedding
```

Pull models from Hugging Face:

```bash
tentgent model pull google/gemma-3-1b-it
tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
tentgent model pull BAAI/bge-reranker-base --capability rerank --revision main
```

`--capability` accepts `chat`, `embedding`, `rerank`,
`audio-transcription`, `audio-speech`, `vision-chat`, `video-understanding`, or
`image-generation`. Chat, embedding, rerank, audio transcription, audio speech,
vision chat, video understanding, and image generation endpoints enforce this
metadata before runtime dispatch.
`audio-transcription` is available through `tentgent transcribe` and the daemon
job API for local safetensors ASR models. `audio-speech` is available through
`tentgent speak` and daemon `POST /v1/audio/speech/job` for local Transformers
text-to-speech models with `wav` output. MLX audio text-to-speech is available
through the direct Python model-runtime path and still needs Rust daemon job
routing.
`vision-chat` is available through `tentgent vision chat` and daemon
`POST /v1/vision/chat` for local safetensors image-plus-text models.
`video-understanding` is available through `tentgent video understand` and
daemon `POST /v1/video/understanding/job` for local video-plus-text models.
`image-generation` is available through
`tentgent image generate` and daemon `POST /v1/images/generations/job` for
local Diffusers text-to-image models and Apple Silicon MFLUX `mlx-diffusion`
models.

List and inspect models:

```bash
tentgent model catalog
tentgent model catalog --capability chat --publisher Qwen
tentgent model catalog --support-level fixture-supported
tentgent model catalog --local --capability embedding
tentgent model ls
tentgent model inspect <model-ref-or-prefix>
tentgent model capability show <model-ref>
tentgent model capability set <model-ref> embedding
tentgent model capability add <model-ref> vision-chat
tentgent model capability remove <model-ref> chat
tentgent model capability verify <model-ref> vision-chat
tentgent model capability proofs <model-ref>
tentgent model capability proof clear <model-ref> chat
```

`model catalog` lists the built-in model-family support catalog before models
are pulled into the local store. Use `--capability`, `--publisher`,
`--support-level`, `--local`, and `--query` filters to narrow the list. Rows
are followed by a pull command template and descriptions for the capabilities
present in the filtered results.
`model ls` keeps the table compact and prints an `Inspect:` command template
below the table. The list also shows a compact support summary such as
`supported chat`, `unknown embedding`, or `failed chat (+1)` when a model has
multiple capability rows. The list omits long source revision hashes; full
source revision and detailed capability support status are shown by
`model inspect` inside the Field/Value table as compact multi-line rows instead
of a wide capability table.
`model inspect` also shows a `catalog` row when built-in model-family records
match the stored source metadata. Catalog matches identify curated fixtures and
major model families, but `verified` and `failed` support statuses still come
only from local proof records.

When no explicit capability or confident Hugging Face metadata is available,
Tentgent keeps the backward-compatible `chat` default and prints a warning.
Use `model capability set`, `add`, or `remove` to correct stored metadata later
without changing `model_ref`. Capability mutations are canonicalized, de-duped,
and rejected when they would leave the model with no capabilities. The legacy
`model set-capability` command remains as a compatibility alias for replacing
the whole capability set with one value.
For MLX models, inspect output also shows `mlx_runtime_family` when the stored
capability maps to a specific MLX runtime family such as `mlx-lm`,
`mlx-vlm`, `mlx-audio`, or `mlx-diffusion`.
Capability proof commands read and write local tuple-aware support proof
records while preserving the legacy latest proof file for compatibility.
Manual `verify` checks stored metadata and backend labeling; local model-bound
server starts record `server-start` proofs after launch success or failure.
Launch-derived proofs include the selected runtime profile id and version when
the server spec has one, so a later profile version is treated as new evidence
instead of silently reusing the old proof.
Use `model capability proof clear <model-ref> <capability>` after fixing a
runtime problem to remove stale local `verified` or `failed` proof evidence for
that capability. This does not remove stored capability metadata or model
content.
`tentgent doctor` also reports local model support summaries as capability
checks. Local model-bound server starts now use the same support status as a
startup gate: `verified` and `supported` are allowed by default, `failed` and
`unsupported` are blocked, and `unknown` or `stale` require an explicit
`--allow-unverified` retry.
Detailed support diagnostics are intentionally kept out of `model ls`.
`model inspect <model-ref>` shows each capability as a multi-line detail row
with `runtime_profile`, `execution_backend`, proof or hint evidence, failure or
stale reason, and a copyable `next_action` when the tuple needs operator work.

For recommended small Hugging Face fixtures, gated-access reminders, and
copy-paste smoke commands, see [model-fixtures.md](./model-fixtures.md).

## Chat

Run one-shot chat:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

Run one-shot chat with an adapter:

```bash
tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Think step by step: what is 12 * 7?" \
  --max-tokens 128
```

Run one-shot embedding inference without starting the daemon:

```bash
tentgent embed <embedding-model-ref> \
  --input "first text" \
  --input "second text" \
  --pretty
```

Run one-shot rerank inference without starting the daemon:

```bash
tentgent rerank <rerank-model-ref> \
  --query "refund policy" \
  --document "first candidate text" \
  --document "second candidate text" \
  --top-n 1 \
  --pretty
```

`tentgent embed` and `tentgent rerank` print JSON with the resolved `model_ref`
and a `data` array matching daemon `/v1/embeddings` and `/v1/rerank` responses.
They are useful for scripts and smoke tests. For repeated traffic, use daemon
REST or a direct local server so the model can stay warm.

## Audio Transcription

Run foreground audio transcription without starting the daemon:

```bash
tentgent transcribe /absolute/path/audio.mp3 \
  --model-ref <audio-transcription-model-ref> \
  --output transcript.txt \
  --format text
```

With `--output`, the command writes only to the requested file and prints a
short completion message. It fails if the output file already exists. Without
`--output`, `text` and `json` formats print to stdout. `vtt` and `srt` are
subtitle formats and require `--output`; they also require backend segment
timestamps.

Pull a small model before running local transcription:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull openai/whisper-tiny.en --capability audio-transcription
```

MP3 and other compressed audio files require `ffmpeg` on `PATH` because the
Transformers ASR pipeline uses it to decode file paths. On macOS:

```bash
brew install ffmpeg
```

`tentgent doctor` reports this as `media decoder ffmpeg`. Missing `ffmpeg`
does not block non-media commands, but local audio/video file jobs should treat
the warning as required setup. The doctor warning prints an install hint for
the current operating system.

For HTTP integrations, send the audio file to the daemon as multipart form
data; the daemon stores the complete file in a job workspace, starts
transcription, and serves result bytes through the workflow result route:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
```

Start a transcription job:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<audio-transcription-model-ref> \
  -F output_format=text \
  -F language=en \
  -F timestamps=false \
  -F file=@/absolute/path/audio.mp3
```

Omit `language` for English-only Whisper checkpoints such as
`openai/whisper-tiny.en`. Use `language` with multilingual checkpoints such as
`openai/whisper-tiny`.

Inspect the job and read result bytes:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>

curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o transcript.txt
```

Supported output formats are `text`, `json`, `vtt`, and `srt`. Reading the
result before completion returns `result_pending`; inspect `/v1/jobs/<job-id>`
for progress or terminal error details. Workspace chunks and temporary files
are internal details. The multipart upload and result reads are
transport-stream-friendly memory boundaries, not realtime model inference. For
the complete HTTP contract, including byte-array multipart upload semantics,
see [api.md](./api.md). The daemon rejects multipart media file parts above
the daemon-wide upload cap with `upload_too_large`; set
`TENTGENT_MEDIA_UPLOAD_MAX_BYTES` before daemon startup to adjust the default
20 MiB cap.

## Audio Speech

Run foreground text-to-speech without starting the daemon:

```bash
tentgent speak \
  --model-ref <audio-speech-model-ref> \
  --text "Hello from Tentgent." \
  --output speech.wav
```

You can also read UTF-8 text from a local file:

```bash
tentgent speak \
  --model-ref <audio-speech-model-ref> \
  --text-file prompt.txt \
  --output speech.wav
```

`tentgent speak` requires `--output`, writes a WAV file, and fails before
running if the output file already exists. It does not print audio bytes to the
terminal. `--format` defaults to `wav`; `wave` is accepted as an alias. `mp3`
is intentionally not supported in M6P.

Pull a small model before running local speech synthesis:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull facebook/mms-tts-eng --capability audio-speech
```

`facebook/mms-tts-eng` is a convenient small Transformers TTS fixture, but its
license is CC-BY-NC 4.0. Accept and evaluate the model license before using it
outside local testing.

For HTTP integrations, send text as JSON. The daemon creates a job, writes the
generated WAV inside the job workspace, and serves result bytes through the
workflow result route:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/speech/job \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<audio-speech-model-ref>",
    "text": "Hello from Tentgent.",
    "output_format": "wav",
    "output_filename": "speech.wav"
  }'
```

Inspect the job and read result bytes:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>

curl -sS \
  'http://127.0.0.1:8790/v1/audio/speech/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o speech.wav
```

Reading the result before completion returns `result_pending`; inspect
`/v1/jobs/<job-id>` for progress or terminal error details. Optional
`language` and `voice` fields are model-aware: when a selected runtime cannot
honor them, the job fails with a clear backend error rather than silently
ignoring the request. Text is limited by
`TENTGENT_AUDIO_SPEECH_MAX_TEXT_BYTES`, defaulting to 64 KiB.

## Vision Chat

Run foreground vision chat without starting the daemon:

```bash
tentgent vision chat /absolute/path/image.png \
  --model-ref <vision-chat-model-ref> \
  --prompt "Describe this image in one sentence." \
  --output answer.md \
  --format md
```

With `--output`, the command writes only to the requested file and prints a
short completion message. It fails if the output file already exists. Without
`--output`, `text` and `md` print the generated answer to stdout; `json` prints
the response envelope.

Pull a small model before running local vision chat:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull HuggingFaceTB/SmolVLM-256M-Instruct --capability vision-chat
```

For HTTP integrations, send the image as multipart form data:

```bash
curl -sS http://127.0.0.1:8790/v1/vision/chat \
  -F model_ref=<vision-chat-model-ref> \
  -F prompt='Describe this image in one sentence.' \
  -F output_format=text \
  -F image=@/absolute/path/image.png
```

Supported input media types are PNG, JPEG, and WebP. The daemon upload route
stores the complete image in a request-scoped temp file, calls the runtime, and
removes the temp file after success or failure. This is a native Tentgent
endpoint. Vision chat image processing uses the `local-model` Python profile
dependencies, including Pillow and torchvision; no system `ffmpeg` install is
needed for PNG, JPEG, or WebP. OpenAI, Claude, and Gemini compatible
multimodal payloads are still text-only rejected until a later compatibility
slice. The same daemon-wide media upload cap applies here; set
`TENTGENT_MEDIA_UPLOAD_MAX_BYTES` before daemon startup to adjust it.

## Video Understanding

Run foreground video understanding without starting the daemon:

```bash
tentgent video understand /absolute/path/video.mp4 \
  --model-ref <video-understanding-model-ref> \
  --prompt "Describe this video briefly." \
  --output answer.txt \
  --format text \
  --sample-fps 0.5 \
  --max-frames 4 \
  --max-frame-edge 384
```

With `--output`, the command writes only to the requested file and prints a
short completion message. It fails if the output file already exists. Without
`--output`, `text` and `md` print the generated answer to stdout; `json` prints
the response envelope, including `sampled_frames`.

Pull a small model before running local video understanding:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull HuggingFaceTB/SmolVLM2-256M-Video-Instruct \
  --capability video-understanding
```

For HTTP integrations, send the video as multipart form data. The daemon
creates a job, writes the uploaded bytes into the job workspace, samples
bounded frames, runs the selected model, and serves result bytes through the
workflow result route:

```bash
curl -sS http://127.0.0.1:8790/v1/video/understanding/job \
  -F model_ref=<video-understanding-model-ref> \
  -F prompt='Describe this video briefly.' \
  -F output_format=text \
  -F sample_fps=0.5 \
  -F max_frames=4 \
  -F max_frame_edge=384 \
  -F file=@/absolute/path/video.mp4
```

Inspect the job and read result bytes:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>

curl -sS \
  'http://127.0.0.1:8790/v1/video/understanding/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o video-understanding.txt
```

Video understanding is batch frame-sampled analysis, not realtime video
streaming. Upload/result reads are transport-stream-friendly memory
boundaries. Video uploads use `TENTGENT_VIDEO_UPLOAD_MAX_BYTES`, defaulting to
512 MiB, instead of the smaller audio/image media cap.

## Image Generation

Run foreground text-to-image generation without starting the daemon:

```bash
tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --prompt "A small ceramic teapot on a wooden table" \
  --output image.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

The command always writes to `--output` and fails before running if that file
already exists. Supported output formats are `png` and `jpg`. Width and height
must be between 64 and 1024 pixels and divisible by 8. Steps must be 1 through
100, and guidance scale must be 0 through 30. Diffusers image generation
defaults to the first available supported device. MLX image-generation models
with `mlx_runtime_family = mlx-diffusion` run through MFLUX on Apple Silicon
macOS after the `local-model` runtime profile is bootstrapped. You can force
one Diffusers command to CPU when debugging Apple MPS or CUDA issues:

```bash
TENTGENT_IMAGE_GENERATION_DEVICE=cpu tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --prompt "A smiling face avatar" \
  --output avatar.png
```

Use one managed image LoRA adapter by importing or pulling it first, then pass
the adapter reference to the same image command:

```bash
tentgent adapter pull <hf-image-lora-repo> \
  --base-model-ref <image-generation-model-ref> \
  --target-capability image-generation \
  --adapter-format diffusers-lora \
  --backend-support diffusers \
  --weight-file pytorch_lora_weights.safetensors \
  --trigger-word "<optional-trigger>"

tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --adapter-ref <image-lora-adapter-ref> \
  --lora-scale 0.8 \
  --prompt "A smiling face avatar, <optional-trigger>" \
  --output avatar.png
```

For MFLUX-backed `mlx-diffusion` models, use
`--adapter-format mlx-diffusion-lora --backend-support mlx-diffusion` and a
Flux-compatible local `.safetensors` weight file. Trigger words are hints only;
Tentgent does not rewrite prompts.

Transform one input image with a prompt:

```bash
tentgent image transform \
  --model-ref <image-generation-model-ref> \
  --input-image input.png \
  --prompt "Turn this into a watercolor illustration" \
  --strength 0.6 \
  --output transformed.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

`tentgent image transform` is foreground-only like `image generate`: it reads
the local `--input-image`, writes only to `--output`, and fails if the output
file already exists. Input images must be PNG, JPEG, or WebP. `--strength`
uses Diffusers image-to-image semantics: `0.0` preserves the input image as
much as possible, while `1.0` lets the model regenerate most of the image.
The same optional `--adapter-ref` and `--lora-scale` flags work for compatible
image LoRA adapters.

Repaint only the white area of one mask image:

```bash
tentgent image inpaint \
  --model-ref <image-generation-model-ref> \
  --input-image input.png \
  --mask-image mask.png \
  --prompt "Replace the masked area with a small ceramic teapot" \
  --strength 1.0 \
  --output inpainted.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

`tentgent image inpaint` is foreground-only like the other image commands. The
base image and mask must be PNG, JPEG, or WebP files. Mask semantics are
`white = repaint` and `black = keep`; Tentgent normalizes the mask to binary
grayscale before runtime execution. The input image and mask must decode to
the same dimensions before the runtime resizes them to the requested output
size. `--strength` defaults to `1.0`, must be `0.0..=1.0`, and uses the same
Diffusers-style denoising meaning as `image transform`.

MLX inpainting requires a Flux Fill-compatible `mlx-diffusion` model; general
Flux text-to-image models are rejected for this workflow instead of guessed
through an incompatible runtime path.

Generate from a prompt plus one typed control image:

```bash
tentgent image control \
  --model-ref <image-generation-model-ref> \
  --control-ref <controlnet-adapter-ref> \
  --control-image control.png \
  --control-kind canny \
  --prompt "A small cabin following the control image structure" \
  --control-strength 1.0 \
  --output controlled.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

`tentgent image control` is foreground-only. It reads the local
`--control-image`, resolves `--control-ref` as a managed ControlNet-style
adapter, and writes only to `--output`. M6O supports `--control-kind canny`.
The control image must already be the control representation for that kind;
Tentgent does not auto-run canny/depth/pose preprocessing in this slice.
`--control-strength` defaults to `1.0` and must be `0.0..=2.0`.
Optional image LoRA still uses `--adapter-ref` and `--lora-scale`.

Pull a tiny Diffusers plumbing fixture before local smoke tests:

```bash
tentgent runtime bootstrap --profile local-model
tentgent model pull hf-internal-testing/tiny-stable-diffusion-pipe --capability image-generation
```

Pull the current MLX image-generation smoke candidate when you have enough disk
and memory for a multi-GiB Apple Silicon test:

```bash
tentgent model pull mlx-community/Flux-1.lite-8B-MLX-Q4 --capability image-generation
```

For HTTP integrations, create an image generation job with JSON:

```bash
curl -sS http://127.0.0.1:8790/v1/images/generations/job \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref":"<image-generation-model-ref>",
    "adapter_ref":"<optional-image-lora-adapter-ref>",
    "lora_scale":0.8,
    "prompt":"A small ceramic teapot on a wooden table",
    "output_format":"png",
    "output_filename":"teapot.png",
    "width":512,
    "height":512,
    "steps":20
  }'
```

Inspect the job, list generated files, and download a file:

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS http://127.0.0.1:8790/v1/images/generations/job/<job-id>/files
curl -sS http://127.0.0.1:8790/v1/images/generations/job/<job-id>/files/teapot.png \
  -o teapot.png
```

Reading result files before completion returns `result_pending`; inspect
`/v1/jobs/<job-id>` for progress or terminal error details. The daemon stores
generated files in the job workspace and exposes only the file listing and file
download APIs.

For HTTP image-to-image integrations, upload file bytes with multipart form
data. The daemon writes the uploaded image into the job workspace before
runtime execution starts; it does not accept client-local image paths:

```bash
curl -sS http://127.0.0.1:8790/v1/images/transforms/job \
  -F image=@/absolute/path/input.png \
  -F model_ref=<image-generation-model-ref> \
  -F prompt='Turn this into a watercolor illustration' \
  -F strength=0.6 \
  -F output_format=png \
  -F output_filename=transformed.png

curl -sS http://127.0.0.1:8790/v1/images/transforms/job/<job-id>/files
curl -sS http://127.0.0.1:8790/v1/images/transforms/job/<job-id>/files/transformed.png \
  -o transformed.png
```

For HTTP inpainting integrations, upload both the base image and mask bytes.
The daemon stores both files in the job workspace before starting the model:

```bash
curl -sS http://127.0.0.1:8790/v1/images/inpaint/job \
  -F image=@/absolute/path/input.png \
  -F mask=@/absolute/path/mask.png \
  -F model_ref=<image-generation-model-ref> \
  -F prompt='Replace the masked area with a small ceramic teapot' \
  -F strength=1.0 \
  -F output_format=png \
  -F output_filename=inpainted.png

curl -sS http://127.0.0.1:8790/v1/images/inpaint/job/<job-id>/files
curl -sS http://127.0.0.1:8790/v1/images/inpaint/job/<job-id>/files/inpainted.png \
  -o inpainted.png
```

For HTTP controlled image generation, upload the control image bytes and pass a
managed ControlNet-style adapter reference separately from any image LoRA:

```bash
curl -sS http://127.0.0.1:8790/v1/images/control/job \
  -F control_image=@/absolute/path/control.png \
  -F model_ref=<image-generation-model-ref> \
  -F control_ref=<controlnet-adapter-ref> \
  -F control_kind=canny \
  -F prompt='A small cabin following the control image structure' \
  -F control_strength=1.0 \
  -F output_format=png \
  -F output_filename=controlled.png \
  -F width=64 \
  -F height=64 \
  -F steps=2 \
  -F seed=1

curl -sS http://127.0.0.1:8790/v1/images/control/job/<job-id>/files
curl -sS http://127.0.0.1:8790/v1/images/control/job/<job-id>/files/controlled.png \
  -o controlled.png
```

Small ControlNet smoke fixtures can be slow or memory-heavy at the default
`512x512` and `20` steps on PyTorch MPS. Use explicit small dimensions for
plumbing tests, then raise quality settings for real models.

## Server

Launch a stable local server proxy:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
tentgent server inspect <server-ref>
```

`--port` is optional. When omitted, Tentgent creates an auto-port server spec
that starts scanning at `8780` each time the server is launched. The first free
port is recorded as the running process `bound_port`; `server ls`, `server ps`,
and daemon health calls use that actual port. When `--port` is provided, that
port is fixed and startup fails if it is unavailable.
`server ls` keeps local model rows compact by showing the model `short_ref` in
the `model` column. Use `server inspect <server-ref>` when the full bound
`model_ref` is needed.

Local model-bound server creation checks the selected capability, runtime
profile availability, and effective support status before saving or launching
the server. `verified` local proofs and `supported` catalog hints are allowed
by default. `failed` and `unsupported` are blocked. `unknown` and `stale` are
blocked unless you explicitly retry with `--allow-unverified`:

```bash
tentgent server run <model-ref> --capability chat --allow-unverified
tentgent server start <server-ref> --allow-unverified
```

When `--capability` is omitted for a local model, Tentgent chooses the server
endpoint family from the model's stored capabilities. The priority is
`video-understanding`, `vision-chat`, `image-generation`, `audio-transcription`,
`audio-speech`, `rerank`, `embedding`, then `chat`. Use `--capability chat`,
`embedding`, `rerank`, `audio-transcription`, `audio-speech`, `vision-chat`,
`video-understanding`, or `image-generation` to override that choice. Local
servers bind the selected model in their Rust proxy spec, so the direct server
request body does not need `model_ref`, `model`, or `model_kind` fields. The
proxy starts or reuses the shared Python model runtime on demand; that Python
runtime may idle-shutdown and be started again on a later request. When set,
`--idle-seconds` becomes the shared Python runtime idle shutdown policy if this
proxy is the process that starts it. Direct Python runtime callers that do not
start a model-bound server may still send explicit `model` and `model_kind`
fields.

For local model-bound servers, `server inspect` includes a `model_support` row
for the server capability selected at creation time. This row reports the
current local proof or catalog-derived support status for the bound model,
selected runtime profile, runtime profile version, execution backend, and
copyable next action for failed, stale, unknown, or unsupported tuples. Local
chat servers also show `runtime_profile` and `runtime_profile_version` when the
server spec records the selected backend profile, such as `local-chat-mlx-v1`.
Runtime profiles are server execution metadata, not dependency bootstrap
profiles; see [server-runtime-profile.md](../contracts/server-runtime-profile.md).
Cloud provider servers do not show local model support because they are bound
to provider-hosted models rather than records in the local model store.

Use `doctor` when you want the same support diagnostics across all stored local
models. `doctor` keeps the main check list compact and places long profile,
backend, failure, and next-action details in the `Details` block.

Call the server:

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Hello there"}
    ],
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

Direct model-server chat is stateless. Do not send `session_ref` or
`max_session_messages` to a server port such as `8780`; those daemon-only fields
belong on daemon `POST /v1/chat` requests, usually port `8790`.

The same local chat server also accepts text-only OpenAI Chat Completions
requests through an ingress adapter. The request `model` is accepted for client
compatibility but the server still uses the bound local model from
`tentgent server run <model-ref>`.

```bash
curl -s http://127.0.0.1:8780/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      {"role": "user", "content": "Hello there"}
    ],
    "max_tokens": 128,
    "temperature": 0.0,
    "stream": false
  }'
```

The same chat server also accepts text-only Claude Messages requests through an
ingress adapter. Claude `max_tokens` is required. The request `model` is
accepted for client compatibility but the server still uses the bound local
model.

```bash
curl -s http://127.0.0.1:8780/v1/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "claude-3-5-sonnet-latest",
    "system": "Answer briefly.",
    "messages": [
      {"role": "user", "content": "Hello there"}
    ],
    "max_tokens": 128,
    "temperature": 0.0,
    "stream": false
  }'
```

The same chat server also accepts text-only Gemini `generateContent` requests
through an ingress adapter. The path model is accepted for client compatibility
but the server still uses the bound local model.

```bash
curl -s http://127.0.0.1:8780/v1beta/models/gemini-2.5-flash:generateContent \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [
      {"role": "user", "parts": [{"text": "Hello there"}]}
    ],
    "generationConfig": {
      "maxOutputTokens": 128,
      "temperature": 0.0
    }
  }'
```

Launch and call a direct local embedding server:

```bash
tentgent server run <embedding-model-ref> \
  --capability embedding \
  --host 127.0.0.1 \
  --port 8781 \
  --lazy-load

curl -s http://127.0.0.1:8781/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{"input":["first text","second text"]}'
```

The same local embedding server also accepts OpenAI-compatible embedding
requests through an ingress adapter. The request `model` is accepted for client
compatibility but the server still uses the bound local model.
Supported local embedding server starts show the selected runtime profile in
`server inspect`, such as `local-embedding-transformers-peft-v1` or
`local-embedding-llama-cpp-v1`. MLX embedding is recognized by the runtime but
does not have a bundled local runtime profile yet, so `server run --capability
embedding` fails before launch for MLX embedding models.

```bash
curl -s http://127.0.0.1:8781/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "text-embedding-3-small",
    "input": ["first text", "second text"],
    "encoding_format": "float"
  }'
```

Launch and call a direct local rerank server:

```bash
tentgent server run <rerank-model-ref> \
  --capability rerank \
  --host 127.0.0.1 \
  --port 8782 \
  --lazy-load

curl -s http://127.0.0.1:8782/v1/rerank \
  -H 'Content-Type: application/json' \
  -d '{"query":"refund policy","documents":["first text","second text"],"top_n":1}'
```

Local servers reject endpoint families that do not match their launch
capability. Image generation chooses the runtime kind from both the bound model
format and the requested image workflow. LoRA tuning is intentionally not a
model-bound server capability; managed tuning runs choose their base model
through the train plan and tuning payload.

Stream a local base-model response with Server-Sent Events:

```bash
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "幫我列三個今天下午安排工作的建議。"}
    ],
    "max_tokens": 160,
    "temperature": 0.2,
    "stream": true
  }'
```

Stream with a compatible local adapter:

```bash
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "請用繁體中文簡短介紹你自己。"}
    ],
    "adapter_ref": "<adapter-ref>",
    "max_tokens": 128,
    "temperature": 0.0,
    "stream": true
  }'
```

Use background server mode:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load --detach
tentgent server ls
tentgent server ps
tentgent server stop <server-ref>
```

Launch a direct cloud provider server:

```bash
tentgent server run openai:gpt-4.1-mini --host 127.0.0.1 --port 8783
tentgent server run anthropic:claude-3-5-sonnet-latest --host 127.0.0.1 --port 8784
tentgent server run gemini:gemini-2.0-flash --host 127.0.0.1 --port 8785
tentgent server run gemini:text-embedding-004 --capability embedding --port 8786
```

Cloud provider servers are Rust workers. They use provider keys from
env/keychain at launch and expose `/v1/chat`, `/v1/chat/completions`,
`/v1/messages`, `/v1/embeddings`, and `/v1/images/generations` when the bound
provider supports that endpoint family. Explicit cloud server capabilities are
accepted for `chat`, `vision-chat`, `embedding`, and `image-generation`;
unsupported provider combinations are rejected at server spec creation.

## Daemon

Start the local daemon process in background mode:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
```

`tentgent daemon start` and `tentgent daemon run --detach` use the same detached
launch path. Loopback daemon binds can run without auth for local development.
To protect daemon `/v1/*` routes, set a local bearer token before starting the
daemon:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
tentgent daemon start --host 127.0.0.1 --port 8790
```

When the token is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"` to every daemon `/v1/*`
request. `GET /healthz` stays public.

Inspect, call, or stop the daemon:

```bash
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz

curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/doctor \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"

curl -sS http://127.0.0.1:8790/v1/daemon/logs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS 'http://127.0.0.1:8790/v1/daemon/logs/stderr?tail_bytes=4096' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"

curl -sS http://127.0.0.1:8790/v1/models \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/adapters \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/datasets \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/jobs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"

curl -sS http://127.0.0.1:8790/v1/chat \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<model-ref>",
    "messages": [{"role": "user", "content": "Say hello in Traditional Chinese."}],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": false
  }'

curl -sS -N http://127.0.0.1:8790/v1/chat/completions \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "<model-ref>",
    "messages": [{"role": "user", "content": "Say hello in Traditional Chinese."}],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'

curl -sS http://127.0.0.1:8790/v1/embeddings \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<embedding-model-ref>",
    "input": ["first text", "second text"]
  }'

curl -sS http://127.0.0.1:8790/v1/embeddings \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "text-embedding-3-small",
    "input": ["first text", "second text"],
    "encoding_format": "float"
  }'

curl -sS http://127.0.0.1:8790/v1/rerank \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<rerank-model-ref>",
    "query": "refund policy",
    "documents": ["first text", "second text"],
    "top_n": 1
  }'

curl -sS -N http://127.0.0.1:8790/v1/messages \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "<model-ref>",
    "max_tokens": 64,
    "messages": [{"role": "user", "content": "Say hello in Traditional Chinese."}],
    "stream": true
  }'

curl -sS -N 'http://127.0.0.1:8790/v1beta/models/<model-ref>:streamGenerateContent' \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "contents": [
      {"role": "user", "parts": [{"text": "Say hello in Traditional Chinese."}]}
    ]
  }'

curl -sS http://127.0.0.1:8790/v1/servers \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"runtime_ref":"openai:gpt-4.1-mini","host":"127.0.0.1","port":8780}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/start \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"wait_ready":true,"timeout_seconds":30}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/health \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/logs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS 'http://127.0.0.1:8790/v1/servers/<server-ref>/logs/stderr?tail_bytes=4096' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/stop \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{}'
curl -sS http://127.0.0.1:8790/v1/daemon/shutdown \
  -X POST \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{}'
tentgent daemon stop
```

For foreground debugging, use:

```bash
tentgent daemon run --host 127.0.0.1 --port 8790
```

The daemon records process metadata under `TENTGENT_HOME/runtime` and exposes
Rust HTTP health/status, store discovery and mutation, controlled server
lifecycle endpoints, background jobs, chat, sessions, and LoRA plan APIs.
Native `/v1/chat`, native `/v1/embeddings`, native `/v1/rerank`, native
`/v1/vision/chat`, OpenAI-compatible `/v1/chat/completions`, Claude-compatible
`/v1/messages`, and Gemini-compatible
`/v1beta/models/{model}:generateContent` adapters are DTO/SSE translators over
kernel use cases. Text chat routes currently reject tools, images, and audio
before calling the model runtime; image-plus-text requests use
`/v1/vision/chat`. Embedding, rerank, and vision chat requests do not create or
mutate sessions.
Log diagnostics endpoints expose fixed daemon/server stdout and stderr paths for
local debugging.
Non-loopback or wildcard daemon binds require `TENTGENT_DAEMON_TOKEN` or the
explicit `--allow-unsafe-bind` flag.
Detached daemon children inherit daemon configuration environment variables,
including `TENTGENT_DAEMON_TOKEN`; local model-server proxy children remove
that token before launch.
`POST /v1/daemon/shutdown` requires `TENTGENT_DAEMON_TOKEN` even on loopback
and stops only the daemon process. It marks active daemon jobs `interrupted` and
runs one retention-aware job workspace sweep, but it does not stop running
model-bound servers.

Inspect and remove managed store entries through the daemon:

```bash
curl -sS http://127.0.0.1:8790/v1/models/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/model","capability":"embedding"}'
curl -sS http://127.0.0.1:8790/v1/models/pull \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/name","revision":null,"capability":"rerank"}'
curl -sS http://127.0.0.1:8790/v1/models/pull/jobs \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/name","revision":null,"capability":"rerank"}'
curl -sS http://127.0.0.1:8790/v1/models/<model-ref>/capabilities \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"set":["embedding"]}'
curl -sS http://127.0.0.1:8790/v1/models/<model-ref>/capabilities/verify \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"capability":"embedding"}'
curl -sS http://127.0.0.1:8790/v1/models/<model-ref>/capabilities/proofs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/adapters/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/adapter","base_model_ref":"<model-ref>"}'
curl -sS http://127.0.0.1:8790/v1/adapters/pull \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"repo_id":"owner/image-lora","base_model_ref":"<image-model-ref>","target_capability":"image-generation","adapter_format":"diffusers-lora","backend_support":["diffusers"],"weight_file":"pytorch_lora_weights.safetensors","trigger_words":["optional trigger"],"recommended_scale":0.8}'
curl -sS http://127.0.0.1:8790/v1/adapters/<adapter-ref>/bind \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"base_model_ref":"<model-ref>"}'
curl -sS http://127.0.0.1:8790/v1/datasets/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/datasets/validate \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/datasets/template \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"task":"support","language":"zh-TW"}'
curl -sS http://127.0.0.1:8790/v1/datasets/synth/jobs \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/generated","brief":"Generate support examples in Traditional Chinese.","split":"train","count":20,"timeout_seconds":300,"retries":1}'
curl -sS http://127.0.0.1:8790/v1/datasets/eval/jobs \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"dataset_ref":"<dataset-ref>","provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/eval-report","max_records":20}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/export \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"output_path":"/absolute/path/on/daemon-host/work-dir"}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/diff \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"right_path":"/absolute/path/on/daemon-host/work-dir"}'
curl -sS http://127.0.0.1:8790/v1/models/<model-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/models/<model-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/adapters/<adapter-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/adapters/<adapter-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/datasets/<dataset-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS -X DELETE http://127.0.0.1:8790/v1/servers/<server-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/jobs \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
```

Import paths are read from the daemon host filesystem, must be absolute, and
may expose local source/store paths in responses. Pull endpoints are synchronous
compatibility calls and may outlive short client timeouts on large downloads.
Use the `/jobs` variants for daemon-side background progress without changing
the synchronous response shape for existing clients.
Dataset validation failures return HTTP `200` with `valid:false`; HTTP `400`
is reserved for malformed daemon requests. Dataset template returns the prompt
body in JSON and does not write a file. Dataset export writes only to a missing
or empty daemon-host directory. Dataset diff returns at most 500 file entries
with `truncated:true` when the underlying diff is larger. Dataset synth/eval job
endpoints create daemon-side background jobs. They can accept direct spec or
dataset content for tool integrations, but may send that selected content to
the configured provider. Failed provider runs return debug artifact paths, not
raw provider output.

Server delete removes a stopped server spec only. Stop a running server before
deleting it. Model and adapter delete may return `409 in_use` when server specs
still reference them.

Create and inspect local sessions from the CLI:

```bash
tentgent session create --title "Planning" --tag draft
tentgent session ls
tentgent session inspect <session-ref>
tentgent session append <session-ref> --role user --content "Hello"
tentgent session append <session-ref> --role user --content "Hello" --compaction-server <server-ref>
tentgent session compact <session-ref> --server <server-ref>
tentgent session messages <session-ref> --tail 100
tentgent session update <session-ref> --title "Planning v2"
tentgent chat <model-ref> --session <session-ref> --message "user:Continue."
tentgent session rm <session-ref>
```

Read and mutate local sessions through the daemon:

```bash
curl -sS http://127.0.0.1:8790/v1/sessions \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS "http://127.0.0.1:8790/v1/sessions/<session-ref>/messages?tail=100" \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/sessions \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"title":"Planning","tags":["draft"]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/messages \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/compact \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"server_ref":"<server-ref>","keep_recent_messages":49}'
```

Session deletion is permanent. Chat remains stateless unless `--session` or
`session_ref` is provided. Session-aware chat serializes turns for a session
while the model response is running so transcript order stays stable. Sessions
are bounded working context: when they would exceed 50 messages, older messages
may be destructively summarized into one `system` summary message.

## Adapters

Import or pull adapters:

```bash
tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
tentgent adapter ls
```

Adapter requests should visibly change answer style when the adapter is compatible with the base model.
Image-generation LoRA adapters should include `--target-capability
image-generation`, a backend such as `diffusers` or `mlx-diffusion`, and
`--weight-file` when the source has more than one `.safetensors` file. The
daemon `/v1/adapters/import`, `/v1/adapters/pull`, and their `/jobs` variants
accept the same image LoRA metadata as JSON fields.

ControlNet-style image control adapters should be imported or pulled as a
separate control adapter, not as an image LoRA:

```bash
tentgent adapter pull <hf-controlnet-repo> \
  --base-model-ref <image-generation-model-ref> \
  --target-capability image-generation \
  --adapter-type controlnet \
  --adapter-format diffusers-controlnet \
  --backend-support diffusers \
  --control-kind canny
```

## Datasets

Import local datasets for training or evaluation:

```bash
tentgent dataset validate /path/to/dataset.jsonl
tentgent dataset validate /path/to/dataset-dir
tentgent dataset template -t chat -l zh-TW -o dataset-template.md
tentgent dataset add /path/to/dataset.jsonl
tentgent dataset add /path/to/dataset-dir
tentgent dataset ls
tentgent dataset inspect <dataset-ref>
tentgent dataset export <dataset-ref> /path/to/work-dir
tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
tentgent dataset diff <dataset-ref> -p /path/to/work-dir
tentgent dataset rm <dataset-ref>
```

A training dataset directory is ready when it contains `train.jsonl`. Optional companions include `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json`.

New chat and tool-use datasets should use the canonical `tentgent.chat.v1` schema in [docs/contracts/dataset-schema.md](../contracts/dataset-schema.md).

Use `dataset template` when you want a paste-ready prompt for OpenAI, Claude, Gemini, or another agent to produce JSONL that should pass `dataset validate`.
Its `--task` and `--language` options are prompt hints only. For example, `--task support` asks the template to prefer support-style examples, and `--language zh-TW` asks for Traditional Chinese content; both still produce the same `tentgent.chat.v1` schema.

Provider-backed `dataset synth` and `dataset eval` use Rust cloud clients:

```bash
tentgent dataset synth --provider gemini --model gemini-2.0-flash \
  --brief "support chat in zh-TW" --output ./generated-dataset --count 20

tentgent dataset eval ./generated-dataset --provider openai --model gpt-4.1-mini \
  --output ./dataset-eval --split all --max-records 20
```

Most common long options have short aliases. Run `tentgent <command> --help` to see them; help always supports `-h`.

To edit a managed dataset, export it to a working directory, edit there, then run `dataset add` again to create a new content-derived reference.

## LoRA Training

Create, inspect, and run a managed LoRA training plan:

```bash
tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
tentgent train lora plan ls
tentgent train lora plan inspect <plan-ref>
tentgent train lora plan rm <plan-ref>
tentgent train lora run <plan-ref>
```

Tentgent auto-selects the backend from the model format: `mlx` models use MLX, `safetensors` models use PEFT, and `gguf` models are blocked for LoRA training.

Common plan overrides: `--rank`, `--learning-rate`, `--batch-size`, `--grad-accum`, `--max-steps`, `--seed`, and `--max-seq-length`.

New LoRA plans mask prompt/context by default: the model still sees system, user, and tool context, but train loss only applies to the final assistant output. Use `--no-mask-prompt` only for plain continuation experiments where role labels and prompt framing should also be trained.

The daemon exposes the same plan-management step without starting training:

```bash
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/preview \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","dataset_ref":"<dataset-ref>","backend":"auto","overrides":{"rank":8,"max_steps":100}}'

curl -sS http://127.0.0.1:8790/v1/train/lora/plans \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","dataset_ref":"<dataset-ref>","backend":"auto"}'

curl -sS http://127.0.0.1:8790/v1/train/lora/plans
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>
curl -sS -X DELETE http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>
```

If `TENTGENT_DAEMON_TOKEN` is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"`. HTTP deletion only removes
plans with zero run records.

Start and monitor a run through the daemon:

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/metrics
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/logs/raw
```

Run start returns after a detached worker process starts. Only one live LoRA run
is allowed at a time in the MVP. Training raw logs may include local paths or
dataset content and are not redacted.
