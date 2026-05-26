# Model Fixtures

Use this guide when you want small models for local smoke tests. `chat`,
`embedding`, `rerank`, `audio-transcription`, `audio-speech`, `vision-chat`,
`video-understanding`, and `image-generation` are runnable endpoint families.
Future media capability values can be stored as model metadata when added, but
they need dedicated runtime endpoints before they are runnable.

## Access Labels

- `public`: the model files are publicly downloadable from Hugging Face.
- `terms`: your Hugging Face account must accept model terms before download.
- `license`: read the model license before using it beyond local smoke tests.
- `metadata-only`: Tentgent accepts this as model metadata, but no endpoint can
  run it yet.
- `cli`: runnable from a foreground Tentgent CLI command.
- `daemon`: runnable from a daemon HTTP route.
- `daemon-job`: run this through a daemon background job and workflow result
  route.
- `planned`: Tentgent does not yet accept this workflow name.
- `internal-test`: useful for plumbing tests, not a product-quality model.

For `terms` models, log in to Hugging Face in a browser, accept the model
terms, then provide an HF token to Tentgent:

```bash
tentgent auth hf set
```

An HF token alone is not enough if the account has not accepted the model
terms.

## Setup

Installed command examples use `tentgent`. In a source checkout, prefix the
same command with:

```bash
cargo run -p tentgent-cli --
```

Prepare local-model dependencies when running local safetensors, Diffusers, or
MLX media models:

```bash
tentgent runtime bootstrap --profile local-model
tentgent runtime status --profile local-model
```

Use an isolated runtime home for smoke tests when you do not want to touch your
normal model store:

```bash
export TENTGENT_HOME=/private/tmp/tentgent-model-smoke
```

## Pull And Inspect

Pull or import models:

```bash
tentgent model pull Qwen/Qwen2.5-0.5B-Instruct --capability chat
tentgent model pull BAAI/bge-small-en-v1.5 --capability embedding
tentgent model pull cross-encoder/ms-marco-MiniLM-L6-v2 --capability rerank

tentgent model add /path/to/local-model --capability chat
```

Inspect and correct metadata:

```bash
tentgent model ls
tentgent model inspect <model-ref-or-prefix>
tentgent model capability set <model-ref-or-prefix> embedding
tentgent model capability add <model-ref-or-prefix> vision-chat
```

Accepted model metadata capability values:

```text
chat
embedding
rerank
audio-transcription
audio-speech
vision-chat
video-understanding
image-generation
```

`chat`, `embedding`, `rerank`, `audio-transcription`, `audio-speech`,
`vision-chat`, `video-understanding`, and `image-generation` have foreground
CLI runtime paths today. `audio-transcription` and `video-understanding` have
daemon file-upload job runtime paths for HTTP integrations. `audio-speech` has
a daemon JSON job runtime path. `vision-chat` has a bounded daemon multipart
route. `image-generation` has daemon job routes and generated-file download
routes. Direct local model-bound server routes are available for chat,
embedding, rerank, audio, vision, video, and image endpoint families.

## Runnable Smoke Commands

Chat:

```bash
tentgent chat <chat-model-ref> \
  --message "user:Say hello in one short sentence." \
  --max-tokens 64
```

Embedding:

```bash
tentgent embed <embedding-model-ref> \
  --input "Rust ownership controls memory." \
  --input "A chocolate cake recipe uses flour." \
  --pretty
```

Rerank:

```bash
tentgent rerank <rerank-model-ref> \
  --query "what is rust ownership" \
  --document "Rust ownership controls memory without a garbage collector." \
  --document "A chocolate cake recipe uses flour and sugar." \
  --document "Borrowing lets Rust check references safely." \
  --top-n 2 \
  --pretty
```

Audio transcription CLI:

```bash
tentgent transcribe /absolute/path/audio.mp3 \
  --model-ref <audio-transcription-model-ref> \
  --output transcript.txt \
  --format text
```

Vision chat CLI:

```bash
tentgent vision chat /absolute/path/image.png \
  --model-ref <vision-chat-model-ref> \
  --prompt "Describe this image in one sentence." \
  --output answer.md \
  --format md
```

Video understanding CLI:

```bash
tentgent video understand /absolute/path/video.mp4 \
  --model-ref <video-understanding-model-ref> \
  --prompt "Describe this video briefly." \
  --output video-understanding.txt \
  --format text \
  --sample-fps 0.5 \
  --max-frames 4 \
  --max-frame-edge 384
```

Image generation CLI:

```bash
tentgent image generate \
  --model-ref <image-generation-model-ref> \
  --adapter-ref <optional-image-lora-adapter-ref> \
  --lora-scale 0.8 \
  --prompt "A small ceramic teapot on a wooden table" \
  --output image.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

Image-to-image transform CLI:

```bash
tentgent image transform \
  --model-ref <image-generation-model-ref> \
  --input-image /absolute/path/input.png \
  --prompt "Turn this into a watercolor illustration" \
  --strength 0.6 \
  --output transformed.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

Masked inpainting CLI:

```bash
tentgent image inpaint \
  --model-ref <image-generation-model-ref> \
  --input-image /absolute/path/input.png \
  --mask-image /absolute/path/mask.png \
  --prompt "Replace the masked area with a small ceramic teapot" \
  --strength 1.0 \
  --output inpainted.png \
  --format png \
  --width 512 \
  --height 512 \
  --steps 20
```

Controlled image generation CLI:

```bash
tentgent image control \
  --model-ref <image-generation-model-ref> \
  --control-ref <controlnet-adapter-ref> \
  --control-image /absolute/path/control.png \
  --control-kind canny \
  --prompt "Follow the control image structure" \
  --control-strength 1.0 \
  --output controlled.png \
  --format png \
  --width 64 \
  --height 64 \
  --steps 2 \
  --seed 1
```

Daemon REST for repeated local tests:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790

curl -sS http://127.0.0.1:8790/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<embedding-model-ref>",
    "input": ["first text", "second text"]
  }'

curl -sS http://127.0.0.1:8790/v1/rerank \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<rerank-model-ref>",
    "query": "refund policy",
    "documents": ["first candidate", "second candidate"],
    "top_n": 1
  }'
```

Audio transcription daemon job for HTTP integrations:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<audio-transcription-model-ref> \
  -F output_format=text \
  -F language=en \
  -F timestamps=false \
  -F file=@/absolute/path/audio.mp3

curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o transcript.txt
```

MP3 inputs require `ffmpeg` on `PATH`. Omit `language` for English-only
Whisper checkpoints such as `openai/whisper-tiny.en`; keep it for multilingual
checkpoints such as `openai/whisper-tiny`. `vtt` and `srt` output require
backend segment timestamps.

Audio speech CLI and daemon job:

```bash
tentgent speak \
  --model-ref <audio-speech-model-ref> \
  --text "Hello from Tentgent." \
  --output /private/tmp/tentgent-speech.wav

curl -sS http://127.0.0.1:8790/v1/audio/speech/job \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref": "<audio-speech-model-ref>",
    "text": "Hello from Tentgent.",
    "output_format": "wav",
    "output_filename": "speech.wav"
  }'

curl -sS \
  'http://127.0.0.1:8790/v1/audio/speech/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o speech.wav
```

Audio speech currently writes WAV only. Optional `language` and `voice` values
are model-aware and may fail when the selected model/runtime cannot honor
them.

Vision chat daemon route:

```bash
curl -sS http://127.0.0.1:8790/v1/vision/chat \
  -F model_ref=<vision-chat-model-ref> \
  -F prompt='Describe this image in one sentence.' \
  -F output_format=text \
  -F image=@/absolute/path/image.png
```

Vision chat accepts PNG, JPEG, and WebP inputs in the native daemon route.
Multipart media uploads share the daemon-wide `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`
cap, which defaults to 20 MiB. Direct OpenAI, Claude, and Gemini compatible
multimodal payloads remain out of scope for this fixture page.

Video understanding daemon job:

```bash
curl -sS http://127.0.0.1:8790/v1/video/understanding/job \
  -F model_ref=<video-understanding-model-ref> \
  -F prompt='Describe this video briefly.' \
  -F output_format=text \
  -F sample_fps=0.5 \
  -F max_frames=4 \
  -F max_frame_edge=384 \
  -F file=@/absolute/path/video.mp4

curl -sS \
  'http://127.0.0.1:8790/v1/video/understanding/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o video-understanding.txt
```

Video understanding accepts one video per job and samples bounded frames before
calling the selected model. Video uploads use
`TENTGENT_VIDEO_UPLOAD_MAX_BYTES`, defaulting to 512 MiB, because video files
are commonly much larger than audio/image fixture inputs.

Image generation daemon job:

```bash
curl -sS http://127.0.0.1:8790/v1/images/generations/job \
  -H 'Content-Type: application/json' \
  -d '{
    "model_ref":"<image-generation-model-ref>",
    "prompt":"A small ceramic teapot on a wooden table",
    "output_format":"png",
    "output_filename":"teapot.png",
    "width":512,
    "height":512,
    "steps":20
  }'

curl -sS http://127.0.0.1:8790/v1/images/generations/job/<job-id>/files
curl -sS http://127.0.0.1:8790/v1/images/generations/job/<job-id>/files/teapot.png \
  -o teapot.png
```

Image transform daemon job:

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

Image inpaint daemon job:

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

Image control daemon job:

```bash
curl -sS http://127.0.0.1:8790/v1/images/control/job \
  -F control_image=@/absolute/path/control.png \
  -F model_ref=<image-generation-model-ref> \
  -F control_ref=<controlnet-adapter-ref> \
  -F control_kind=canny \
  -F prompt='Follow the control image structure' \
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

Use the explicit `64x64` and `2` step settings for tiny ControlNet smoke
fixtures. The daemon defaults remain `512x512` and `20` steps for normal image
jobs, which can be too heavy for small plumbing fixtures on PyTorch MPS.

## Current Fixture Models

These rows are for local smoke tests, not product defaults.

| Capability | Candidate | Access | Pull command | Notes |
| --- | --- | --- | --- | --- |
| `chat` | [`HuggingFaceTB/SmolLM-135M-Instruct`](https://huggingface.co/HuggingFaceTB/SmolLM-135M-Instruct) | `public` | `tentgent model pull HuggingFaceTB/SmolLM-135M-Instruct --capability chat` | Very small Apache-2.0 text-generation smoke target. |
| `chat` | [`Qwen/Qwen2.5-0.5B-Instruct`](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct) | `public` | `tentgent model pull Qwen/Qwen2.5-0.5B-Instruct --capability chat` | Stronger 0.5B Apache-2.0 chat target. |
| `chat` | [`google/gemma-3-1b-it`](https://huggingface.co/google/gemma-3-1b-it) | `terms`, `license` | `tentgent model pull google/gemma-3-1b-it --capability chat` | Requires accepting Google's Gemma terms on Hugging Face before pull. |
| `chat` | [`meta-llama/Llama-3.2-1B-Instruct`](https://huggingface.co/meta-llama/Llama-3.2-1B-Instruct) | `terms`, `license` | `tentgent model pull meta-llama/Llama-3.2-1B-Instruct --capability chat` | Requires accepting Meta Llama terms on Hugging Face before pull. |
| `embedding` | [`sentence-transformers/paraphrase-MiniLM-L3-v2`](https://huggingface.co/sentence-transformers/paraphrase-MiniLM-L3-v2) | `public` | `tentgent model pull sentence-transformers/paraphrase-MiniLM-L3-v2 --capability embedding` | Fast 384-dim embedding smoke target. |
| `embedding` | [`BAAI/bge-small-en-v1.5`](https://huggingface.co/BAAI/bge-small-en-v1.5) | `public` | `tentgent model pull BAAI/bge-small-en-v1.5 --capability embedding` | MIT-licensed 384-dim retrieval embedding candidate. |
| `rerank` | [`cross-encoder/ms-marco-MiniLM-L6-v2`](https://huggingface.co/cross-encoder/ms-marco-MiniLM-L6-v2) | `public` | `tentgent model pull cross-encoder/ms-marco-MiniLM-L6-v2 --capability rerank` | 22.7M parameter rerank smoke target. |
| `rerank` | [`mixedbread-ai/mxbai-rerank-xsmall-v1`](https://huggingface.co/mixedbread-ai/mxbai-rerank-xsmall-v1) | `public` | `tentgent model pull mixedbread-ai/mxbai-rerank-xsmall-v1 --capability rerank` | Apache-2.0 reranker, about 70.8M parameters. |
| `rerank` | [`BAAI/bge-reranker-base`](https://huggingface.co/BAAI/bge-reranker-base) | `public` | `tentgent model pull BAAI/bge-reranker-base --capability rerank` | Heavier MIT-licensed accuracy target for rerank tests. |

## M6 Media Fixture Models

Audio transcription candidates can run through `tentgent transcribe` and the
daemon job route. Vision chat candidates can run through `tentgent vision chat`
and daemon `POST /v1/vision/chat`. Video understanding candidates can run
through `tentgent video understand` and daemon
`POST /v1/video/understanding/job`. Image generation candidates can run through
`tentgent image generate`, `tentgent image transform`, `tentgent image
inpaint`, `tentgent image control`, daemon
`POST /v1/images/generations/job`, daemon `POST /v1/images/transforms/job`,
daemon `POST /v1/images/inpaint/job`, and daemon
`POST /v1/images/control/job`.
Audio speech candidates can run through `tentgent speak` and daemon
`POST /v1/audio/speech/job` when their backend is supported. Other candidates
are for metadata and contract planning. Pulling them with their media
`--capability` values records model intent only; it does not make unsupported
workflow families runnable.
For `mlx-community/*` repos, the same capability flag also records
`mlx_runtime_family` when it can be inferred. `mlx-vlm` can run native
`vision-chat` on Apple Silicon after the `local-model` runtime profile is
bootstrapped. `mlx-audio` can run native `audio-transcription` on Apple
Silicon after the `local-model` runtime profile is bootstrapped.
`mlx-diffusion` can run native `image-generation` through MFLUX on Apple
Silicon after the `local-model` runtime profile is bootstrapped. MLX audio TTS
can run through the direct Python model-runtime path; Rust daemon job routing
still needs to be wired to that runtime.
Image-generation LoRA adapters can be used with the same CLI and daemon image
job surfaces after they are imported or pulled into the managed adapter store.
Use explicit `--target-capability image-generation`, backend support, and
`--weight-file` metadata for adapter repos with ambiguous `.safetensors` files.
ControlNet-style image control adapters must be imported or pulled separately
with `--adapter-type controlnet --adapter-format diffusers-controlnet
--backend-support diffusers --control-kind canny`.

Masked inpainting additionally requires an inpainting-capable Diffusers
pipeline or a Flux Fill-compatible MLX diffusion model. The general
`Flux-1.lite` MFLUX fixture is text-to-image/image-to-image oriented and should
be expected to fail fast on inpainting until a fill-compatible fixture is
pinned.

| Metadata capability | Candidate | Access | Pull command | Notes |
| --- | --- | --- | --- | --- |
| `audio-transcription` | [`openai/whisper-tiny.en`](https://huggingface.co/openai/whisper-tiny.en) | `public`, `cli`, `daemon-job` | `tentgent model pull openai/whisper-tiny.en --capability audio-transcription` | English ASR, safetensors, about 38M parameters. |
| `audio-transcription` | [`openai/whisper-tiny`](https://huggingface.co/openai/whisper-tiny) | `public`, `cli`, `daemon-job` | `tentgent model pull openai/whisper-tiny --capability audio-transcription` | Multilingual tiny Whisper checkpoint, about 39M parameters. |
| `audio-transcription` | [`mlx-community/whisper-tiny-asr-fp16`](https://huggingface.co/mlx-community/whisper-tiny-asr-fp16) | `public`, `mlx-audio`, `cli`, `daemon-job` | `tentgent model pull mlx-community/whisper-tiny-asr-fp16 --capability audio-transcription` | Small Apple Silicon MLX audio smoke target; inspect should show `mlx_runtime_family = mlx-audio`. |
| `audio-transcription` | [`mlx-community/whisper-tiny-mlx`](https://huggingface.co/mlx-community/whisper-tiny-mlx) | `public`, `mlx-audio`, `processor-metadata-warning` | `tentgent model pull mlx-community/whisper-tiny-mlx --capability audio-transcription` | Older MLX Whisper package; current `mlx-audio` may fail because the repo lacks Hugging Face processor metadata. |
| `audio-transcription` | [`mlx-community/whisper-tiny-fp16`](https://huggingface.co/mlx-community/whisper-tiny-fp16) | `public`, `mlx-audio`, `processor-metadata-warning` | `tentgent model pull mlx-community/whisper-tiny-fp16 --capability audio-transcription` | Older MLX Whisper package; prefer `whisper-tiny-asr-fp16` for current `mlx-audio` smoke tests. |
| `audio-speech` | [`facebook/mms-tts-eng`](https://huggingface.co/facebook/mms-tts-eng) | `public`, `license`, `transformers-tts`, `cli`, `daemon-job` | `tentgent model pull facebook/mms-tts-eng --capability audio-speech` | English VITS TTS, about 36M parameters; CC-BY-NC 4.0. Verified M6P small TTS smoke target after license review. |
| `audio-speech` | [`mlx-community/Kokoro-82M-bf16`](https://huggingface.co/mlx-community/Kokoro-82M-bf16) | `public`, `mlx-audio`, `direct-runtime` | `tentgent model pull mlx-community/Kokoro-82M-bf16 --capability audio-speech` | MLX TTS smoke target, about 371 MiB in the managed store; direct Python model-runtime smoke verified with `voice=af_heart` and 24 kHz WAV output. |
| `audio-speech` | [`suno/bark-small`](https://huggingface.co/suno/bark-small) | `public`, `candidate`, `transformers-tts` | `tentgent model pull suno/bark-small --capability audio-speech` | MIT-licensed TTS pipeline candidate; heavier than MMS-TTS and not the first smoke fixture. |
| `vision-chat` | [`HuggingFaceTB/SmolVLM-256M-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM-256M-Instruct) | `public`, `cli`, `daemon` | `tentgent model pull HuggingFaceTB/SmolVLM-256M-Instruct --capability vision-chat` | Small image+text to text model for Transformers VQA/captioning smoke tests. |
| `vision-chat` | [`mlx-community/SmolVLM-256M-Instruct-bf16`](https://huggingface.co/mlx-community/SmolVLM-256M-Instruct-bf16) | `public`, `mlx-vlm`, `cli`, `daemon` | `tentgent model pull mlx-community/SmolVLM-256M-Instruct-bf16 --capability vision-chat` | Small Apple Silicon MLX VLM smoke target; inspect should show `mlx_runtime_family = mlx-vlm`. |
| `video-understanding` | [`HuggingFaceTB/SmolVLM2-256M-Video-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM2-256M-Video-Instruct) | `public`, `cli`, `daemon-job` | `tentgent model pull HuggingFaceTB/SmolVLM2-256M-Video-Instruct --capability video-understanding` | Small Apache-2.0 video-aware VLM fixture. M6Q samples bounded frames with the local-model Python decoder before calling the model. |
| `image-generation` | [`hf-internal-testing/tiny-stable-diffusion-pipe`](https://huggingface.co/hf-internal-testing/tiny-stable-diffusion-pipe) | `public`, `internal-test`, `cli`, `daemon-job` | `tentgent model pull hf-internal-testing/tiny-stable-diffusion-pipe --capability image-generation` | Diffusers plumbing fixture only; not product-quality output. |
| `image-generation` | [`hf-internal-testing/tiny-stable-diffusion-pipe-no-safety`](https://huggingface.co/hf-internal-testing/tiny-stable-diffusion-pipe-no-safety) | `public`, `internal-test`, `cli`, `daemon-job`, `controlnet-smoke-base` | `tentgent model pull hf-internal-testing/tiny-stable-diffusion-pipe-no-safety --capability image-generation` | Tiny Diffusers base model verified with `hf-internal-testing/tiny-controlnet`; use `64x64` and `2` steps for smoke tests. |
| `image-generation` | [`segmind/tiny-sd`](https://huggingface.co/segmind/tiny-sd) | `public`, `cli`, `daemon-job` | `tentgent model pull segmind/tiny-sd --capability image-generation` | Tiny Stable Diffusion-style model; larger than the internal fixture and useful for follow-up smoke tests. |
| `image-generation` | [`mlx-community/Flux-1.lite-8B-MLX-Q4`](https://huggingface.co/mlx-community/Flux-1.lite-8B-MLX-Q4) | `public`, `mlx-diffusion`, `cli`, `daemon-job`, `large` | `tentgent model pull mlx-community/Flux-1.lite-8B-MLX-Q4 --capability image-generation` | Apple Silicon MFLUX smoke candidate, about 7.5 GiB. Inspect should show `mlx_runtime_family = mlx-diffusion`. |

No small, project-verified public image LoRA fixture is pinned yet. When adding
one, record the base model, adapter pull command, required `--weight-file`,
trigger-word hints, and whether it works through Diffusers, MFLUX, or both.

Verified tiny ControlNet smoke pair:

```bash
tentgent model pull hf-internal-testing/tiny-stable-diffusion-pipe-no-safety \
  --capability image-generation

tentgent adapter pull hf-internal-testing/tiny-controlnet \
  --base-model-ref <tiny-base-model-ref> \
  --target-capability image-generation \
  --adapter-type controlnet \
  --adapter-format diffusers-controlnet \
  --backend-support diffusers \
  --control-kind canny

tentgent image control \
  --model-ref <tiny-base-model-ref> \
  --control-ref <tiny-controlnet-adapter-ref> \
  --control-image test-data/test_image.png \
  --control-kind canny \
  --prompt "a tiny clean icon following the control image structure" \
  --output /private/tmp/tentgent-m6o-controlled-64.png \
  --width 64 \
  --height 64 \
  --steps 2 \
  --seed 1
```

This fixture is for plumbing only, not output quality. The uploaded control
image is resized by the runtime to the requested dimensions, and M6O does not
auto-run canny preprocessing.

## Notes

- The current Hugging Face pull path downloads full snapshots. Some repos may
  include ONNX or extra assets that inflate local size.
- Add allow-pattern support later if smoke fixtures become too large.
- Keep future media fixture commands marked as metadata-only until runtime
  support exists.
