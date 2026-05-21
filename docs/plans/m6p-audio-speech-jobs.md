# M6P Audio Speech Jobs

Status: implemented in M6P for the Transformers text-to-speech path; MLX audio
TTS remains a planned backend until a stable local `mlx-audio` TTS path is
verified.

M6P adds the first native `audio-speech` workflow: text-to-speech that produces
one durable audio artifact. The slice should reuse the kernel audio feature
area and daemon job workspace foundation, but it should expose a product
workflow rather than lower-level spool or chunk APIs.

## Goal

Allow users to synthesize one audio file from text:

```bash
tentgent speak \
  --model-ref <audio-speech-model-ref> \
  --text "Hello from Tentgent." \
  --output speech.wav
```

HTTP integrations should use a daemon JSON job route:

```http
POST /v1/audio/speech/job
Content-Type: application/json
```

The result should be downloaded through a workflow-owned route:

```http
GET /v1/audio/speech/job/{job_id}/result
```

## Scope

In scope:

- One local model with `audio-speech` capability.
- One text input.
- Foreground CLI execution through kernel use cases.
- Daemon JSON job creation and job-owned result artifact storage.
- `wav` output as the first supported format.
- Optional `language` and `voice` request fields when the selected runtime can
  support them.
- Clear early failures for unsupported language, unsupported voice, unsupported
  output format, unsupported backend, unreadable model, and unsafe output file
  paths.
- Transformers text-to-speech backend as the first baseline path.
- MLX audio TTS investigation and implementation only if the installed
  `mlx-audio` runtime exposes a stable local TTS API and a small fixture can be
  verified.

Out of scope:

- Realtime speech streaming.
- Speech-to-speech.
- Voice cloning.
- Multi-speaker mixing.
- SSML.
- Forced alignment, phoneme output, word timings, subtitles, or transcript
  sidecars.
- `mp3` output.
- Direct `tentgent server` audio speech routes.
- OpenAI-compatible `/v1/audio/speech` server API.
- Generic spool/upload/result workspace APIs.

## Product Decisions

- Keep the model capability as `audio-speech`.
- Add a foreground CLI command named `tentgent speak`.
- Use `POST /v1/audio/speech/job` for daemon jobs.
- Use `GET /v1/audio/speech/job/{job_id}/result` for artifact download.
- Do not add a path-input JSON route. TTS input is text, not a user file.
- The CLI must require `--output` and must fail if the output file already
  exists.
- The daemon should write generated audio only inside the job workspace.
- The first output format is `wav`.
- `flac` is allowed as a later extension if a backend can produce it or a
  local encoder boundary is approved.
- `mp3` remains deferred until encoder dependency and licensing boundaries are
  approved.
- `language` and `voice` are model-aware options. If supplied for a model that
  cannot honor them, fail early with a clear error instead of silently ignoring
  them.
- Text length is bounded by one configurable daemon/CLI limit, for example
  `TENTGENT_AUDIO_SPEECH_MAX_TEXT_BYTES`, with a conservative default.

## Request Contract

CLI:

```bash
tentgent speak \
  --model-ref <MODEL_REF> \
  --text "Hello from Tentgent." \
  --output speech.wav \
  [--format wav] \
  [--language en] \
  [--voice <VOICE>]
```

Optional text-file form:

```bash
tentgent speak \
  --model-ref <MODEL_REF> \
  --text-file prompt.txt \
  --output speech.wav
```

Rules:

- Exactly one of `--text` or `--text-file` is required.
- `--output` must be a local file path.
- Existing output files are rejected.
- `--format` defaults to `wav`.
- CLI does not print raw audio bytes to the terminal.

Daemon job:

```json
{
  "model_ref": "<audio-speech-model-ref>",
  "text": "Hello from Tentgent.",
  "output_format": "wav",
  "output_filename": "speech.wav",
  "language": "en",
  "voice": "default"
}
```

Fields:

- `model_ref`: required full ref, unique short ref, or supported alias.
- `text`: required non-empty UTF-8 text.
- `output_format`: optional, defaults to `wav`.
- `output_filename`: optional file name only, defaults to `speech.wav`.
- `language`: optional model-aware language hint.
- `voice`: optional model-aware voice or speaker hint.

The daemon should reject unknown fields.

Result route:

```http
GET /v1/audio/speech/job/{job_id}/result?cursor=0&max_chunks=32
```

The route should mirror audio transcription result semantics:

- `409 result_pending` before the first result chunk is available.
- `409 job_failed`, `job_interrupted`, or `job_canceled` for terminal jobs
  without an artifact.
- `404 result_not_found` for successful jobs with no result file.
- `Content-Type: audio/wav` for `wav`.
- `Content-Disposition` with the output file name.
- Cursor headers for chunked result reads.

## Kernel Plan

Extend `src/tentgent-kernel/src/features/audio/` without introducing CLI or
daemon naming into the kernel.

Domain additions:

- `AudioSpeechOutputFormat`
  - `Wav`
  - extension `wav`
  - media type `audio/wav`
- `AudioSpeechBackend`
  - `TransformersTextToSpeech`
  - `MlxAudio` only if M6P verifies a stable TTS call path
- `ResolvedAudioSpeechTarget`
- `AudioSpeechRequest`
  - resolved target
  - text
  - output path
  - output format
  - optional language
  - optional voice
- `AudioSpeechResponse`
  - output format
  - media type
  - output path
  - total bytes
  - sample rate when known

Use cases:

- Add `AudioSpeechPreparationRequest`.
- Add `AudioSpeechPreparationResult`.
- Add `AudioSpeechExecutionResult`.
- Add `AudioSpeechPreparationUseCase`.
- Add `AudioSpeechUseCase`.
- Place implementation in `features/audio/usecases/speech.rs`.

Ports and infra:

- Add an audio speech model resolver beside transcription resolver logic.
- Require model capability `audio-speech`.
- Safetensors models route to Transformers TTS.
- MLX `mlx-audio` models route only when a TTS backend implementation is
  verified; otherwise return a specific unsupported-backend error.
- Add a runtime port implementation that calls a Python one-shot command such
  as `tentgent-audio-speech`.

## Python Runtime Plan

Add a new runtime module:

- `python/tentgent-daemon/src/tentgent_daemon/runtime/audio_speech.py`

Responsibilities:

- Load model records.
- Validate `audio-speech` capability.
- Resolve the backend.
- Normalize output format.
- Validate text length.
- Write WAV output.
- Normalize backend audio payloads into PCM WAV.

Add a new one-shot CLI:

- `python/tentgent-daemon/src/tentgent_daemon/cli/audio_speech.py`
- package script: `tentgent-audio-speech`

Backends:

- Extend `backends/base.py` with `AudioSpeechBackend`.
- Extend `backends/__init__.py` with `create_audio_speech_backend`.
- Add `TransformersPeftAudioSpeechBackend` using the Transformers
  `text-to-speech` pipeline.
- Reuse installed `torch` / `transformers` local-model dependencies.
- Prefer stdlib WAV writing if practical to avoid adding an encoder dependency.
- If backend output is a float array/tensor, clamp to `[-1.0, 1.0]`, convert to
  signed 16-bit PCM, and write a mono WAV unless the backend clearly returns
  multi-channel audio.
- Add an MLX audio TTS probe only after inspecting the installed `mlx-audio`
  API. If no stable local TTS API is found, add a clear planned-backend error
  and keep MLX TTS as a follow-up.

## Daemon Plan

Keep public APIs workflow-level:

- Add `POST /v1/audio/speech/job`.
- Add `GET /v1/audio/speech/job/{job_id}/result`.
- Keep `/v1/jobs`, `/v1/jobs/{job_id}`, cancel, and delete as generic job
  status/control surfaces only.

Implementation placement:

- Put new speech-specific handler code in
  `src/tentgent-daemon/src/handlers/rest/audio/speech.rs`.
- Avoid growing audio `mod.rs` with new feature logic.
- Share safe filename, result response, model alias, and text limit helpers
  with transcription where the extraction is small and low-risk.
- If broad audio handler refactoring becomes noisy, keep the refactor minimal
  and defer full transcription module migration.

Job behavior:

- Create `JobKind::audio_speech` if the current job kind vocabulary does not
  already have one.
- Label jobs as `synthesize speech`.
- Target section should be `audio`, reference should be the model ref.
- Store result bytes in the job workspace result stream.
- Declare one result file with the chosen output filename.
- Mark progress with one file total/done and final byte count.

## CLI Plan

Add a foreground command:

- `src/tentgent-cli/src/cli/commands/speak.rs`
- `src/tentgent-cli/src/cli/speak.rs`

Behavior:

- Read `--text` directly or read UTF-8 from `--text-file`.
- Reject empty text.
- Reject existing output path before runtime work starts.
- Resolve `model_ref` through kernel use cases.
- Execute the audio speech use case directly, not through daemon.
- Print a short success message with output path, format, sample rate when
  known, and byte count.

## Documentation Plan

Update user-facing docs:

- `docs/user/README.md`
- `docs/user/commands.md`
- `docs/user/api.md`
- `docs/user/model-fixtures.md`
- `docs/user/runtime.md`
- `docs/user/version.md`

Document:

- `tentgent speak` examples.
- `POST /v1/audio/speech/job` JSON shape.
- `GET /v1/audio/speech/job/{job_id}/result`.
- `wav` only for M6P.
- `mp3` deferred.
- `language` and `voice` are model-aware and may fail.
- Recommended smoke fixture and license notes.

## Test Plan

Rust unit tests:

- `AudioSpeechOutputFormat` parse/display/default filename/media type.
- Speech backend routing for `safetensors` and rejected non-speech
  capabilities.
- Language/voice options pass through request preparation.
- Runtime command arguments include text, output path, output format, language,
  and voice when present.
- CLI parse test for `tentgent speak`.
- CLI output preflight rejects existing output files.

Daemon tests:

- `POST /v1/audio/speech/job` accepts valid JSON and returns `202`.
- Missing text returns `400`.
- Unknown fields return `400`.
- Invalid output filename returns `400`.
- Result before completion returns `409 result_pending`.
- Result download returns audio media type and cursor headers.

Python tests:

- Runtime plan validates `audio-speech` capability.
- Text length validation.
- WAV writer produces a readable WAV header.
- Transformers TTS backend normalizes common output shapes.
- Unsupported language/voice errors are clear.
- MLX planned-backend error, if no stable TTS API is implemented.

Smoke:

- Pull `facebook/mms-tts-eng` with `--capability audio-speech`.
- Run `tentgent speak` with short text and verify a non-empty WAV.
- Run daemon `POST /v1/audio/speech/job`, poll `/v1/jobs/{job_id}`, download
  result, and verify file type.
- Record whether the model requires license acknowledgement or is unsuitable as
  a permissive default.
- If a practical MLX TTS fixture is verified, add a second smoke. Otherwise,
  record MLX TTS as planned and keep M6P complete for the Transformers path.

## Execution Steps

1. Add kernel `audio-speech` domain, resolver, use-case, and runtime port types.
2. Add Python audio speech runtime module and one-shot command.
3. Implement the Transformers TTS backend and WAV writer.
4. Add CLI `tentgent speak`.
5. Add daemon speech job route and result route.
6. Add tests for kernel, CLI, daemon, and Python runtime behavior.
7. Run a small Transformers TTS smoke test.
8. Investigate MLX audio TTS API; implement if stable, otherwise document the
   planned-backend status.
9. Update user/API/runtime/model-fixture/version docs.
10. Update roadmap status after verification.

## Acceptance Criteria

- `tentgent speak` writes one WAV file from text with a local `audio-speech`
  model.
- `POST /v1/audio/speech/job` creates a daemon job from JSON text input.
- `GET /v1/audio/speech/job/{job_id}/result` returns the generated WAV through
  workflow-owned result semantics.
- CLI and daemon reject unsupported output formats, unsafe output filenames,
  empty text, and incompatible model capabilities.
- Transformers TTS path has unit tests and at least one smoke-tested small
  model.
- MLX TTS is either implemented with smoke evidence or explicitly marked as a
  planned backend with clear user-facing errors.
- Docs clearly state that M6P is artifact-job TTS, not realtime speech
  streaming and not `mp3`.

## Implementation Notes

Implemented:

- Kernel `audio-speech` domain, resolver, use-case, runtime port, and
  `JobKind::audio_speech`.
- Python `tentgent-audio-speech` one-shot runtime entrypoint.
- Transformers `text-to-speech` backend with stdlib PCM16 WAV writing.
- Foreground `tentgent speak` CLI.
- Daemon JSON `POST /v1/audio/speech/job` and result route
  `GET /v1/audio/speech/job/{job_id}/result`.
- User docs, API docs, fixture docs, runtime notes, and roadmap updates.

Smoke evidence:

- Pulled `facebook/mms-tts-eng` with `--capability audio-speech`; stored short
  ref `120fdb6241f4`, primary format `safetensors`, size `277.2 MiB`.
- `tentgent speak --model-ref 120fdb6241f4 --text "Hello from Tentgent M6P."
  --output /private/tmp/tentgent-m6p-speech-20260521.wav` wrote a non-empty
  WAV file.
- `file /private/tmp/tentgent-m6p-speech-20260521.wav` reported RIFF/WAVE,
  Microsoft PCM, 16-bit mono, 16000 Hz.
- Daemon `POST /v1/audio/speech/job` completed job
  `job-1779372608592455000-0`, wrote `m6p-speech.wav`, and result download
  produced the same RIFF/WAVE PCM shape.

MLX TTS remains a planned backend. The current MLX audio backend loads
dependencies but returns a clear planned-backend error for `audio-speech`
execution until a stable local `mlx-audio` TTS API and small fixture are
verified.
