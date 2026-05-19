# M6E Audio Transcription CLI And Large-File Hardening

Status: implemented.

Depends on:

- [M6C audio transcription daemon MVP](./m6c-audio-transcription-daemon-mvp.md)
- [M6D audio transcription file upload jobs](./m6d-audio-transcription-file-stream-job-input.md)

## Goal

Make audio transcription usable from the `tentgent` CLI without requiring users
to call daemon HTTP routes by hand:

```bash
tentgent transcribe /path/to/audio.mp3 \
  --model-ref <audio-transcription-model-ref> \
  --output transcript.txt \
  --format text
```

The first M6E slice should be a foreground, file-to-file command. It should not
register itself as a durable job, hide workspace paths from users, or expose
spool/chunk operations.

## Product Contract

Command:

```text
tentgent transcribe <AUDIO_PATH>
  --model-ref <MODEL_REF>
  [--output <OUTPUT_PATH>]
  [--format text|json|vtt|srt]
  [--language <LANGUAGE>]
  [--timestamps]
  [--home <HOME>]
```

Output behavior:

- If `--output` is provided, write only to that file and print a concise
  completion message such as:

```text
transcription written: transcript.txt
```

- If `--output` is omitted:
  - `text` may print the transcript to stdout.
  - `json` may print JSON to stdout.
  - `vtt` and `srt` require `--output`.

Format meanings:

- `text`: plain transcript text.
- `json`: structured transcript output from the runtime.
- `vtt`: WebVTT subtitle/caption file, usually `.vtt`, with timed cues for
  web/video players.
- `srt`: SubRip subtitle file, usually `.srt`, with numbered timed subtitle
  blocks.
- `vtt` and `srt` require segment-level timestamps from the backend. Selecting
  either format should imply timestamp extraction; if the selected runtime/model
  cannot return segment timings, fail with a clear unsupported-format message.

Default behavior:

- `--format` defaults to `text`.
- The default output filename, when a command path needs one, should derive
  from the input stem and format extension.
- Users provide an audio or audio/video container file. They should not need to
  pre-decode to PCM.

## Non-Goals

- Do not add a public spool/upload/chunk API.
- Do not change the M6D multipart daemon API.
- Do not make foreground CLI commands appear in `GET /v1/jobs`.
- Do not implement realtime ASR or partial transcript streaming.
- Do not add `tentgent server` audio transcription routes in this slice.
- Do not solve full bounded-window ASR for every large audio file. This slice
  should add user-facing guardrails and leave deeper windowing/runtime changes
  for a later hardening slice if needed.
- Do not implement `--detach` in M6E. If the CLI process fails, users can rerun
  the command. Detached audio jobs remain available through the daemon API.

## Execution Model Decision

Implementation: direct foreground runtime.

- Reuse the existing kernel audio transcription use case from the CLI process.
- Resolve the runtime home and Python runtime in read/create mode consistent
  with existing one-shot local model CLI commands.
- Resolve the model through the existing audio transcription model resolver.
- Invoke the existing Python audio transcription runtime client with input path,
  output path, output format, optional language, and timestamps.

Rationale:

- It matches `tentgent embed` and `tentgent rerank`: one-shot local command,
  no daemon dependency.
- It avoids hidden durable job records.
- It gives clear terminal behavior and writes the requested output path
  directly.

CLI rule:

- Except for explicit daemon-management commands, CLI workflows should depend
  on kernel use cases directly and should not require the daemon to be running.

## Daemon API Boundary

M6E is a CLI slice, but it must preserve the daemon API decisions made in M6D:

- `POST /v1/audio/transcriptions/job` remains the canonical daemon job route
  for HTTP integrations.
- The multipart `file` field is a transport-stream-friendly upload. Clients may
  stream the request body, and the daemon writes received bytes into the job
  workspace instead of loading the full upload into memory.
- The upload is still one logical audio input. `file` must appear exactly once.
  Multiple audio files should be submitted as multiple jobs, or merged by the
  caller before upload when one combined transcript is intended.
- Transport streaming is not model streaming. The worker starts ASR after the
  logical input file is complete and passes the daemon-internal file path to the
  runtime.
- Result reads remain workflow-owned. The current cursor/chunk result route
  avoids requiring one full result read. Future large artifact routes may use
  HTTP response-body streaming or range reads, but should still stay behind
  feature-owned result routes instead of exposing generic workspace/chunk APIs.

## Large-File Hardening Scope

M6E should add first-layer guardrails, not pretend every ASR backend is fully
streaming:

- Validate that the input path exists, is absolute or resolvable to an absolute
  local path, and is a file.
- Report input size before execution when useful.
- Warn when the file is large enough that the current backend may take a long
  time or use substantial memory.
- Surface `ffmpeg`/decoder missing errors with doctor-style guidance.
- Preserve backend errors for unsupported language, media decode failure, or
  unsupported output format with clear CLI context.
- Keep successful transcript output atomic enough for normal use: write to a
  temporary file in the destination directory where practical, then rename to
  the requested output path.
- Do not delete user-provided input files.

Large-file root cause:

- The model does not consume arbitrary uploaded byte chunks. For audio, the
  workflow is container bytes or a file path, then decode to waveform/features,
  then model-specific time windows.
- Whisper-style ASR commonly operates on fixed time windows internally. OpenAI
  Whisper documents that `transcribe()` reads the file and processes audio with
  a sliding 30-second window. Hugging Face ASR pipelines accept file paths,
  bytes, raw waveforms, or sampled arrays, and expose chunk/stride controls for
  some long-form use cases.
- Splitting a compressed MP3/MP4 file by byte ranges is not a safe ASR strategy:
  byte ranges may not be independently decodable, and model context boundaries
  are time/audio-feature boundaries, not file-byte boundaries.
- For complete-file transcription, users normally provide a full media file and
  the runtime/pipeline handles decoding and any time-windowing it supports.
- For live dictation or live translation, the right design is a separate
  realtime/streaming transport and runtime port, not this foreground batch CLI.
- Images have a similar constraint: vision models generally need a complete
  decoded image, then resize/crop/tile before inference. Byte chunking image
  files is not useful as model context.
- Video is the media class that most clearly needs bounded decode/sampling:
  workers should sample frames/clips from a complete container instead of
  loading the whole file.

M6E large-file behavior:

- Warn on large compressed input files, but do not fail solely because of size.
- The first warning threshold is 100 MiB.
- The warning text must explain that decoded audio and model windows can use
  more memory than the compressed file size suggests.
- Deeper runtime windowing knobs, such as explicit `chunk_length_s` or
  `stride_length_s`, are deferred until the Python runtime contract is updated
  and tested per backend.

Research references:

- OpenAI Whisper documents `transcribe()` as reading the entire file and
  processing audio with a sliding 30-second window:
  <https://github.com/openai/whisper/blob/main/README.md#python-usage>.
- Hugging Face Transformers ASR pipeline documents inputs as file paths,
  audio-file bytes, raw waveform arrays, or sampled raw-audio dictionaries; its
  pipeline source exposes `chunk_length_s` / `stride_length_s` as audio
  chunking controls:
  <https://huggingface.co/docs/transformers/main_classes/pipelines#transformers.AutomaticSpeechRecognitionPipeline>.
  <https://raw.githubusercontent.com/huggingface/transformers/main/src/transformers/pipelines/automatic_speech_recognition.py>.
- MDN documents WebVTT as timed text tracks for media elements with `.vtt`
  files:
  <https://developer.mozilla.org/en-US/docs/Web/API/WebVTT_API/Web_Video_Text_Tracks_Format>.
- Library of Congress documents SRT as a text-based subtitle/caption format
  played alongside video or audio:
  <https://www.loc.gov/preservation/digital/formats/fdd/fdd000569.shtml>.

## Execution Plan

### 1. CLI Surface

- Add `TranscribeCommand` under `src/tentgent-cli/src/cli/commands/`.
- Add a top-level `Commands::Transcribe` variant.
- Add parsing tests in `src/tentgent-cli/src/cli/mod.rs` for:
  - required input path
  - required `--model-ref`
  - optional `--output`
  - `--format`
  - `--language`
  - `--timestamps`
  - `--home`
- Add user-facing help text that frames the command as foreground local
  transcription.

### 2. CLI Handler

- Add `src/tentgent-cli/src/cli/transcribe.rs`.
- Build a small CLI audio kernel similar to `embed.rs` and `rerank.rs`:
  - `StdRuntimeLayoutResolver`
  - `StdPythonRuntimeResolver`
  - `StdRuntimeExecutableResolver`
  - `FileModelCatalogStore`
  - `StdModelCatalogReadUseCase`
  - `StdAudioTranscriptionModelResolver`
  - `PythonAudioTranscriptionBatchClient`
  - `StdAudioTranscriptionUseCase`
- Map CLI fields to `AudioTranscriptionPreparationRequest`.
- Use `LayoutResolveMode::Create` if the audio runtime path needs to create
  output parent directories or runtime-home support paths; otherwise prefer the
  narrowest existing mode that still works.

### 3. Output Path Rules

- Parse `--format` into `AudioTranscriptionOutputFormat`.
- If `--output` is omitted:
  - allow stdout for `text`
  - allow stdout for `json`
  - reject `vtt` and `srt` with a message asking for `--output`
- If `--output` is provided:
  - reject output paths that are directories
  - create parent directories only if this is already consistent with local CLI
    behavior; otherwise fail with a clear message
  - fail if the output file already exists
- If `--format vtt` or `--format srt` is selected, request segment timestamps
  from the runtime even when `--timestamps` is not passed explicitly.
- If the runtime returns transcript text without segment timings for `vtt` or
  `srt`, fail instead of writing misleading untimed subtitles.
- Do not add `--overwrite` in M6E.

### 4. Input And Dependency Guardrails

- Canonicalize the input path before runtime execution.
- Reject non-files before runtime execution.
- Detect input size and warn at 100 MiB or larger.
- Keep doctor's `ffmpeg` install hints as the source of truth where possible.
- When runtime errors mention missing `ffmpeg` or decode failure, wrap them
  with concise user guidance.
- Preserve language fallback behavior from M6C/M6D for English-only Whisper
  checkpoints where the backend rejects language hints.

### 5. User Documentation

Update:

- `docs/user/commands.md`
- `docs/user/model-fixtures.md`
- `docs/user/api.md` only if CLI behavior affects API guidance
- `docs/user/version.md`
- `docs/plans/capability-first-release-roadmap.md`
- this plan's status and completion notes after implementation

Document:

- Foreground CLI does not create durable jobs.
- Use daemon `POST /v1/audio/transcriptions/job` when an HTTP integration wants
  a job and result route.
- Daemon upload/result streaming is a transport and memory boundary, not
  realtime model inference.
- Audio transcription daemon uploads accept exactly one logical `file`; multiple
  files should be multiple jobs or caller-merged input.
- `--output` behavior.
- `ffmpeg` expectations.
- Current large-file limitations and warning behavior.

### 6. Tests

CLI parsing tests:

- `tentgent transcribe audio.mp3 --model-ref abc --output transcript.txt`
- `--format json`
- `--language en`
- `--timestamps`
- `--home /tmp/tentgent`

CLI validation tests:

- missing `--model-ref`
- unsupported format
- `vtt`/`srt` without `--output`
- `vtt`/`srt` when runtime output lacks segment timestamps
- output exists if overwrite is not approved
- input path not found
- large input warning rendering

Kernel/audio tests:

- Reuse existing audio use case tests where possible.
- No new daemon tests are required because M6E is a foreground CLI workflow.

Smoke test:

```bash
tentgent model pull openai/whisper-tiny.en --capability audio-transcription
tentgent transcribe test-data/we_go_up.mp3 \
  --model-ref <audio-transcription-model-ref> \
  --output /private/tmp/tentgent-transcript.txt \
  --format text
```

Expected terminal output:

```text
transcription written: /private/tmp/tentgent-transcript.txt
```

## Likely Files

Rust CLI:

- `src/tentgent-cli/src/cli/commands/mod.rs`
- `src/tentgent-cli/src/cli/commands/transcribe.rs`
- `src/tentgent-cli/src/cli/mod.rs`
- `src/tentgent-cli/src/cli/transcribe.rs`

Kernel audio, only if small helper extraction is needed:

- `src/tentgent-kernel/src/features/audio/domain.rs`
- `src/tentgent-kernel/src/features/audio/usecases/transcription.rs`

Docs:

- `docs/user/commands.md`
- `docs/user/model-fixtures.md`
- `docs/user/version.md`
- `docs/plans/capability-first-release-roadmap.md`
- `docs/plans/m6e-audio-transcription-cli-and-large-file-hardening.md`

## Verification

Required local checks:

```bash
cargo fmt
cargo check --workspace
cargo test -p tentgent-cli transcribe
cargo test -p tentgent-kernel audio
cargo test --workspace
uv run python -m unittest discover -s tests
```

Recommended local smoke, when the model is available:

```bash
cargo run -p tentgent-cli -- transcribe test-data/we_go_up.mp3 \
  --model-ref <audio-transcription-model-ref> \
  --output /private/tmp/tentgent-transcript.txt \
  --format text
```

Review target:

- A user can run one command from local audio file to local transcript file,
  with predictable output behavior, no hidden durable job, and clear guidance
  for decoder or large-file failures.

## Completion Notes

- Added foreground `tentgent transcribe <AUDIO_PATH> --model-ref <MODEL_REF>`
  with optional `--output`, `--format`, `--language`, `--timestamps`, and
  `--home`.
- Kept the command independent from daemon jobs. The CLI resolves kernel audio
  use cases directly and runs the Python audio transcription batch entrypoint
  once.
- Added output protection: requested output files must not already exist, and
  successful file output is written through a temporary path before becoming
  visible at the requested path.
- Allowed stdout only for `text` and `json`; `vtt` and `srt` require
  `--output`.
- Made subtitle output require backend segment timestamps. `vtt` and `srt`
  imply timestamp extraction, and runtime output without timestamp chunks now
  fails instead of creating misleading untimed subtitles.
- Added large compressed input warning at 100 MiB and decoder-oriented runtime
  error guidance.

Verification completed:

- `cargo fmt`
- `cargo check --workspace`
- `cargo test -p tentgent-cli transcribe`
- `cargo test -p tentgent-kernel audio`
- `cargo test --workspace`
- `uv run python -m unittest discover -s tests`
- Smoke-tested `tentgent transcribe test-data/we_go_up.mp3 --model-ref
  9e9bbd1515bc --output /private/tmp/tentgent-m6e-smoke-transcript-20260520.txt
  --format text`.
- Smoke-tested stdout JSON output and `vtt` output with segment timestamps.
- Verified an existing output file is rejected before runtime execution.
