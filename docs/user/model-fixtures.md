# Model Fixtures

Use this guide when you want small models for local smoke tests. `chat`,
`embedding`, and `rerank` are runnable endpoint families. M6A media capability
values can be stored as model metadata, but they do not have runtime endpoints
yet.

## Access Labels

- `public`: the model files are publicly downloadable from Hugging Face.
- `terms`: your Hugging Face account must accept model terms before download.
- `license`: read the model license before using it beyond local smoke tests.
- `metadata-only`: Tentgent accepts this as model metadata, but no endpoint can
  run it yet.
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

Prepare local-model dependencies when running local safetensors models:

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
tentgent model set-capability <model-ref-or-prefix> embedding
```

Accepted model metadata capability values:

```text
chat
embedding
rerank
audio-transcription
audio-speech
vision-chat
image-generation
```

Only `chat`, `embedding`, and `rerank` have CLI, daemon, and direct server
runtime paths today. The media values are metadata-only until M6B/M6C implement
their payload, artifact, and runtime contracts.

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

## M6A Metadata Fixture Models

These candidates are for metadata and contract planning. Pulling them with
their media `--capability` values records model intent only; it does not make
audio, image, or video inference available yet.

| Metadata capability | Candidate | Access | Pull command | Notes |
| --- | --- | --- | --- | --- |
| `audio-transcription` | [`openai/whisper-tiny.en`](https://huggingface.co/openai/whisper-tiny.en) | `public`, `metadata-only` | `tentgent model pull openai/whisper-tiny.en --capability audio-transcription` | English ASR, safetensors, about 38M parameters. |
| `audio-transcription` | [`openai/whisper-tiny`](https://huggingface.co/openai/whisper-tiny) | `public`, `metadata-only` | `tentgent model pull openai/whisper-tiny --capability audio-transcription` | Multilingual tiny Whisper checkpoint, about 39M parameters. |
| `audio-speech` | [`facebook/mms-tts-eng`](https://huggingface.co/facebook/mms-tts-eng) | `public`, `license`, `metadata-only` | `tentgent model pull facebook/mms-tts-eng --capability audio-speech` | English VITS TTS, about 36M parameters; CC-BY-NC 4.0. |
| `audio-speech` | [`suno/bark-small`](https://huggingface.co/suno/bark-small) | `public`, `metadata-only` | `tentgent model pull suno/bark-small --capability audio-speech` | MIT-licensed TTS pipeline candidate; heavier than MMS-TTS. |
| `vision-chat` | [`HuggingFaceTB/SmolVLM-256M-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM-256M-Instruct) | `public`, `metadata-only` | `tentgent model pull HuggingFaceTB/SmolVLM-256M-Instruct --capability vision-chat` | Small image+text to text model for VQA/captioning contract tests. |
| future video understanding | [`HuggingFaceTB/SmolVLM2-256M-Video-Instruct`](https://huggingface.co/HuggingFaceTB/SmolVLM2-256M-Video-Instruct) | `public`, `planned` | no command until video workflow name is approved | Keep out of the first native endpoint unless video payload handling is approved. |
| `image-generation` | [`hf-internal-testing/tiny-stable-diffusion-pipe`](https://huggingface.co/hf-internal-testing/tiny-stable-diffusion-pipe) | `public`, `internal-test`, `metadata-only` | `tentgent model pull hf-internal-testing/tiny-stable-diffusion-pipe --capability image-generation` | Diffusers plumbing fixture only; not product-quality output. |
| `image-generation` | [`segmind/tiny-sd`](https://huggingface.co/segmind/tiny-sd) | `public`, `metadata-only` | `tentgent model pull segmind/tiny-sd --capability image-generation` | Tiny Stable Diffusion-style model; requires a Diffusers/artifact contract. |

## Notes

- The current Hugging Face pull path downloads full snapshots. Some repos may
  include ONNX or extra assets that inflate local size.
- Add allow-pattern support later if smoke fixtures become too large.
- Keep media fixture commands marked as metadata-only until runtime support
  exists.
