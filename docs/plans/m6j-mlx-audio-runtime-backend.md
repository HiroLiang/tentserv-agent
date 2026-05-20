# M6J MLX Audio Runtime Backend

Status: implemented and smoke-tested.

Depends on:

- [M6C audio transcription daemon MVP](./m6c-audio-transcription-daemon-mvp.md)
- [M6D audio transcription file upload jobs](./m6d-audio-transcription-file-stream-job-input.md)
- [M6E audio transcription CLI and large-file hardening](./m6e-audio-transcription-cli-and-large-file-hardening.md)
- [M6H MLX multimodal backend foundation](./m6h-mlx-multimodal-backend-foundation.md)
- [M6I MLX vision chat backend](./m6i-mlx-vision-chat-backend.md)

## Goal

Make the existing native `audio-transcription` workflow route to Apple Silicon
MLX audio models when a stored model advertises:

```text
primary_format = mlx
mlx_runtime_family = mlx-audio
model_capabilities = [audio-transcription]
```

M6J is backend parity work, not a new product surface. It should mirror the M6I
shape: keep the user-facing command and daemon routes unchanged, then add a
dedicated MLX runtime adapter behind the existing kernel audio use case.

## Product Decision

M6J should implement MLX audio transcription as a parallel backend to the
existing Transformers ASR backend.

Existing surfaces remain canonical:

```bash
tentgent transcribe /absolute/path/audio.mp3 \
  --model-ref <mlx-audio-model-ref> \
  --output /absolute/path/transcript.txt
```

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<mlx-audio-model-ref> \
  -F file=@/absolute/path/audio.mp3
```

```text
GET /v1/audio/transcriptions/job/{job_id}/result
```

No new daemon route, generic media route, standalone `mlx-audio` server route,
or OpenAI-compatible audio route should be added in this slice.

Read-only inference should not add a model-level concurrency lock by default.
Audio transcription loads model weights and reads input audio; it should follow
the existing CLI/daemon execution model and let operators control concurrency
through process/job deployment choices. A later resource-coordination slice can
add model leases for tasks that mutate model data or need exclusive runtime
state, but M6J should not block multiple read-only transcription tasks merely
because they reference the same model.

## Runtime Facts

As of 2026-05-20, the most relevant local runtime candidate is `mlx-audio`.
The package documents STT, TTS, and speech-to-speech families, but M6J should
use only the STT path needed by Tentgent `audio-transcription`.

Audio family terms:

- STT means speech-to-text. This maps to Tentgent `audio-transcription`.
- TTS means text-to-speech. This maps to a future `audio-speech` workflow, not
  M6J.
- STS means speech-to-speech. This produces audio from audio and remains out of
  scope.

Model names and Hugging Face tags are useful hints, not proof of compatibility.
M6J should treat Whisper-style MLX audio models as candidates because they align
with the existing transcription contract, then verify compatibility by running
the runtime against a real local model and `test-data/we_go_up.mp3`.

Observed runtime entry points:

- Whisper STT examples use:

```python
from mlx_audio.stt.generate import generate_transcription

result = generate_transcription(
    model="mlx-community/whisper-large-v3-turbo-asr-fp16",
    audio="audio.wav",
)
print(result.text)
```

- Other ASR families use load-and-generate style APIs:

```python
from mlx_audio.stt import load

model = load("mlx-community/Qwen3-ASR-0.6B-8bit")
result = model.generate("audio.wav", language="English")
print(result.text)
```

Sources:

- [`mlx-audio` GitHub](https://github.com/Blaizzy/mlx-audio)
- [`mlx-audio` PyPI](https://pypi.org/project/mlx-audio/)
- [`mlx-community/whisper-tiny-asr-fp16`](https://huggingface.co/mlx-community/whisper-tiny-asr-fp16)
- [`mlx-community/whisper-tiny-mlx`](https://huggingface.co/mlx-community/whisper-tiny-mlx)
- [`mlx-community/whisper-tiny-fp16`](https://huggingface.co/mlx-community/whisper-tiny-fp16)

## Implementation Results

M6J was implemented as a backend-only slice. No user-facing command or daemon
route changed.

Implemented paths:

- Kernel capability readiness now includes a dedicated `MlxAudio` backend kind.
- Doctor reports `backend mlx-audio` separately.
- Kernel audio resolver maps `ModelFormat::Mlx` plus
  `MlxRuntimeFamily::Audio` to `AudioTranscriptionBackend::MlxAudio`.
- Python router maps `primary_format = "mlx"` plus
  `mlx_runtime_family = "mlx-audio"` to `BackendKind.MLX_AUDIO`.
- Python backend factory now returns `MlxAudioTranscriptionBackend` for
  `BackendKind.MLX_AUDIO`.
- `python/tentgent-daemon/src/tentgent_daemon/backends/mlx_audio.py` loads
  `record.variant_source_path`, calls the `mlx-audio` STT model, normalizes
  text/timestamp results, and writes through the shared audio writer.
- The Python `local-model` extra now includes `mlx-audio` on Apple Silicon.

Smoke result:

- `mlx_audio` imported successfully through `uv run --extra local-model`.
- `mlx-community/whisper-tiny-asr-fp16` pulled successfully with
  `--capability audio-transcription`.
- Model inspect showed:
  - `short_ref = 228446a341e4`
  - `primary_format = mlx`
  - `mlx_runtime_family = mlx-audio`
  - `model_capabilities = audio-transcription`
  - `backend_support = dependency-gated: requires MLX audio Python packages such as mlx and mlx-audio`
- CLI text output succeeded with:

```bash
cargo run -p tentgent-cli -- transcribe test-data/we_go_up.mp3 \
  --model-ref 228446a341e4 \
  --output /private/tmp/tentgent-m6j-mlx-audio-asr-fp16.txt \
  --format text
```

- CLI JSON and VTT timestamp outputs succeeded after avoiding `mlx-audio`
  verbose stdout so the Python entrypoint keeps returning clean JSON to Rust.
- Daemon multipart upload/result route succeeded against an isolated daemon on
  `127.0.0.1:8792`; job `job-1779292599643163000-0` reached `succeeded`, and
  the existing result route returned transcript text.

Validation completed:

```bash
cargo fmt
cargo check --workspace
cargo test --workspace
cd python/tentgent-daemon
uv run python -m unittest discover -s tests
uv run --with ruff ruff check src tests
git diff --check
```

Smoke caveat:

- `mlx-community/whisper-tiny-mlx` and
  `mlx-community/whisper-tiny-fp16` pull and record `mlx_runtime_family =
  mlx-audio`, but current `mlx-audio` fails to run them from the Tentgent local
  model path because those repos lack Hugging Face processor metadata such as
  `preprocessor_config.json`. Prefer
  `mlx-community/whisper-tiny-asr-fp16` for current smoke tests.
- Tentgent does not synthesize or patch processor/tokenizer metadata for MLX
  audio models. A model must include the metadata required by its selected
  runtime loader, or the backend returns an explicit unsupported package-layout
  error.

## Scope

Implement:

- MLX audio dependency gating and doctor visibility.
- Kernel audio backend routing for `ModelFormat::Mlx` plus
  `MlxRuntimeFamily::Audio`.
- Python runtime routing from `BackendKind.MLX_AUDIO` to a new audio backend.
- A Python `MlxAudioTranscriptionBackend` that adapts `mlx-audio` STT output to
  Tentgent's existing transcript output writer.
- CLI and daemon smoke tests using the existing audio entry points.
- User docs and model fixture recommendations for small MLX Whisper candidates.

Keep:

- Existing Transformers ASR behavior for safetensors models.
- Existing audio job workspace and result routes.
- Existing CLI output-path behavior and large-file warnings.
- Existing `text`, `json`, `vtt`, and `srt` output format names.

## Non-Goals

- Do not add `audio-speech` execution. M6J may record whether `mlx-audio` looks
  mature enough for a future M6P speech slice, but it should not implement TTS.
- Do not add speech-to-speech, source separation, enhancement, diarization,
  forced alignment, audio understanding, or live microphone streaming.
- Do not add video input handling.
- Do not add generic chunk-to-model streaming. M6J still consumes one logical
  audio file path, matching the existing audio contract.
- Do not call `mlx_audio.server`; Tentgent owns daemon lifecycle, API shape,
  job state, and result files.
- Do not accept multiple input files in one transcription job.
- Do not add model-level read locks for inference-only jobs. If a deployment
  wants stricter GPU or memory concurrency, that should be introduced as a
  later configurable runtime capacity policy.

## Execution Plan

### 1. Dependency And Capability Probe

- Add `mlx-audio` to the Python `local-model` optional dependency for Apple
  Silicon macOS only.
- Add `BackendKind::MlxAudio` to kernel capability readiness.
- Probe Python modules:

```text
mlx
mlx_audio
```

- Mark `MlxAudio` unsupported on non-Apple-Silicon platforms, same as
  `MlxVlm`.
- Update doctor labels so users see a separate `backend mlx-audio` check.
- Keep bootstrap guidance on the existing `local-model` profile:

```bash
tentgent runtime bootstrap --profile local-model
```

### 2. Kernel Audio Backend Selection

- Extend `AudioTranscriptionBackend` with:

```rust
MlxAudio
```

- Replace the current format-only selector with a family-aware selector:

```rust
AudioTranscriptionBackend::from_model_format_and_mlx_family(
    metadata.primary_format,
    metadata.mlx_runtime_family,
)
```

- Mapping rules:

```text
safetensors                         -> transformers-asr
mlx + mlx_runtime_family = mlx-audio -> mlx-audio
mlx + no family                     -> reject for audio transcription
mlx + mlx-lm                        -> reject for audio transcription
mlx + mlx-vlm                       -> reject for audio transcription
mlx + mlx-diffusion                 -> reject for audio transcription
gguf                                -> reject
diffusers                           -> reject
```

- Preserve the existing capability gate: a model must still advertise
  `audio-transcription`.

### 3. Python Runtime Router

- Change `resolve_audio_transcription_backend()` so:

```text
primary_format = safetensors -> BackendKind.TRANSFORMERS_PEFT
primary_format = mlx and mlx_runtime_family = mlx-audio -> BackendKind.MLX_AUDIO
```

- Call `ensure_backend_supported("mlx_audio")` before returning
  `BackendKind.MLX_AUDIO`.
- Keep clear errors for unsupported MLX families rather than falling through to
  a misleading generic format error.

### 4. Python MLX Audio Backend

Add:

```text
python/tentgent-daemon/src/tentgent_daemon/backends/mlx_audio.py
```

Implement:

```python
class MlxAudioTranscriptionBackend(AudioTranscriptionBackend):
    def load(self, record: StoredModelRecord) -> None: ...
    def transcribe(self, request: AudioTranscriptionRequest) -> AudioTranscriptionResult: ...
    def release(self) -> None: ...
```

Backend behavior:

- Load from `record.variant_source_path` so Tentgent uses the already-pulled
  model store, not an implicit Hugging Face download.
- Do not pass Hugging Face repository ids to the runtime backend after model
  resolution. `mlx-audio` examples often accept repo ids and may use their own
  cache/download path; Tentgent backends must use the managed local model path
  to preserve model-store identity, revision pinning, permissions, and
  reproducibility.
- Prefer a stable `mlx-audio` STT API after a short runtime probe.
- Start with Whisper-style ASR because it directly matches
  `audio-transcription`.
- Do not implement ASR-family-specific options beyond Tentgent's current
  request fields unless needed for smoke correctness.
- Keep `language` behavior aligned with the existing safetensors path: pass it
  when supported; if the selected model/runtime rejects it, fallback only when
  the error is a known unsupported-language condition, otherwise return a clear
  runtime error.
- Request timestamps when `request.timestamps` is true or when output format is
  `vtt` / `srt`.
- Treat VTT/SRT as timestamp-dependent formats. If the runtime result does not
  include usable segment or word timestamps, return the same explicit subtitle
  timestamp error shape as the existing audio writer. Do not invent synthetic
  subtitle timings.
- Normalize raw output into the existing writer shape:

```python
{
    "text": "...",
    "chunks": [
        {"text": "...", "timestamp": (start_seconds, end_seconds)}
    ],
}
```

- Use `write_audio_transcription_output()` so text, JSON, VTT, and SRT output
  remain consistent with the Transformers backend.
- Map missing Python package errors through `missing_profile_dependency()`.

### 5. Backend Factory

- Update `create_audio_transcription_backend()` so
  `BackendKind.MLX_AUDIO` returns `MlxAudioTranscriptionBackend`.
- Keep `BackendKind.TRANSFORMERS_PEFT` unchanged.

### 6. CLI And Daemon Behavior

No new command or route should be added.

CLI expected behavior:

```bash
tentgent transcribe test-data/we_go_up.mp3 \
  --model-ref <mlx-audio-ref> \
  --output /private/tmp/tentgent-m6j-transcript.txt \
  --format text
```

Daemon expected behavior:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<mlx-audio-ref> \
  -F output_format=text \
  -F file=@test-data/we_go_up.mp3
```

The returned `job_id` should be readable through the existing result route.
Pending, failed, canceled, deleted, and GC behavior remain owned by the
existing job subsystem.

### 7. Tests

Kernel tests:

- Audio backend mapping accepts `mlx + mlx-audio`.
- Audio resolver accepts an MLX audio model with `audio-transcription`.
- Audio resolver rejects MLX chat, VLM, diffusion, and missing-family models.
- Capability probe reports `MlxAudio` separately.
- Doctor includes `backend mlx-audio`.

Python tests:

- Router returns `BackendKind.MLX_AUDIO` for MLX audio model records.
- Router calls `ensure_backend_supported("mlx_audio")`.
- Router rejects other MLX media families for audio transcription.
- Backend factory creates `MlxAudioTranscriptionBackend`.
- Backend adapter test uses fake `mlx-audio` deps and proves:
  - local model path is passed to the runtime
  - audio input path is passed to the runtime
  - text output is normalized
  - timestamp chunks are mapped when present
  - output writing still goes through the shared audio writer

Regression tests:

- Existing safetensors Whisper path still resolves to Transformers ASR.
- Existing CLI parser tests remain unchanged.
- Existing daemon audio job tests remain unchanged unless expected backend
  labels need updates.

### 8. Smoke Plan

Dependency smoke:

```bash
cd python/tentgent-daemon
uv run --extra local-model python -c "import mlx_audio; print('mlx_audio import ok')"
```

Primary small model candidates:

```bash
tentgent model pull mlx-community/whisper-tiny-asr-fp16 \
  --capability audio-transcription
```

Older MLX Whisper packages can be pulled for metadata inspection but are not
the recommended runtime smoke target for current `mlx-audio`:

```bash
tentgent model pull mlx-community/whisper-tiny-mlx \
  --capability audio-transcription

tentgent model pull mlx-community/whisper-tiny-fp16 \
  --capability audio-transcription
```

CLI smoke:

```bash
cargo run -p tentgent-cli -- transcribe test-data/we_go_up.mp3 \
  --model-ref <mlx-audio-ref> \
  --output /private/tmp/tentgent-m6j-mlx-audio.txt \
  --format text
```

Daemon smoke:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<mlx-audio-ref> \
  -F output_format=text \
  -F file=@test-data/we_go_up.mp3
```

Then poll:

```bash
curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/job/<job-id>/result?cursor=0&max_chunks=32'
```

Smoke checks should explicitly record:

- Whether the MLX audio runtime accepted `record.variant_source_path` directly.
- Whether `text` output works.
- Whether `json` includes normalized text and any timestamp chunks returned by
  the runtime.
- Whether `vtt`/`srt` work with the selected smoke model or correctly fail with
  a timestamp-required error.
- Whether `--language` behaves consistently with the existing safetensors path.

### 9. Follow-Up Resource Coordination

M6J should not introduce default locks for read-only model inference. However,
future slices should consider daemon-owned model resource coordination for
operations that mutate model state or need exclusive access.

Potential later work:

- Add a `ModelResourceCoordinator` in daemon runtime state.
- Represent leases as read-only inference leases and exclusive mutation leases.
- Let multiple read-only leases coexist by default.
- Allow future configurable capacity limits per backend or model family when
  operators want to prevent GPU/CPU memory pressure.
- Require exclusive leases for model removal, model conversion, quantization,
  adapter merge, or other tasks that write into managed model directories.
- Surface blocked exclusive operations as clear conflict errors instead of
  deleting or mutating a model that active jobs are reading.

This is not required for M6J because the MLX audio transcription path reads
weights and writes only job/CLI output files.

### 10. Documentation Updates

- `docs/contracts/platform-backends.md`
  - move `mlx-audio` from `planned` to `dependency-gated` after runtime smoke.
- `docs/user/runtime.md`
  - describe `mlx-audio` as the Apple Silicon audio transcription path after
    implementation.
- `docs/user/model-fixtures.md`
  - add MLX Whisper tiny candidates and mark larger ASR/TTS models as optional
    or future.
- `docs/user/version.md`
  - update known limits only after the backend is smoke-tested.
- `docs/plans/capability-first-release-roadmap.md`
  - mark M6J implemented only after CLI and daemon smoke pass.

### 11. Validation

Required before closing M6J:

```bash
cargo fmt
cargo check --workspace
cargo test --workspace
cd python/tentgent-daemon
uv run python -m unittest discover -s tests
uv run --with ruff ruff check src tests
git diff --check
```

Runtime smoke should be recorded in this plan before marking it implemented.

## Acceptance Criteria

- `mlx-audio` is a separate backend family, not a hidden branch of `mlx-lm`.
- Existing safetensors audio transcription still works.
- Existing CLI and daemon audio APIs work unchanged with MLX audio models.
- `tentgent model inspect` shows `mlx_runtime_family = mlx-audio` for MLX audio
  models pulled with `--capability audio-transcription`.
- Doctor reports `backend mlx-audio` readiness separately.
- At least one small MLX Whisper candidate is pulled and smoke-tested on Apple
  Silicon with `test-data/we_go_up.mp3`.
- The roadmap does not claim `audio-speech`, live streaming, source separation,
  or video support from this slice.
- M6J does not add a default same-model inference lock. Any future locking or
  runtime capacity limit is recorded as separate resource-coordination work.
