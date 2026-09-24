# Audio Transcription And Speech

Prepare the [local-model runtime profile](../runtime.md#media-runtime-dependencies), a model with the matching capability, and any input files. See [file and HTTP rules](./README.md#file-and-http-media-rules) before choosing CLI or daemon execution.

## Examples And Common Operations

### Audio Transcription

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

### Audio Speech

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

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `transcribe` | `-m, --model-ref <MODEL_REF>` | Stored Tentgent audio-transcription model reference to run |
| `transcribe` | `-o, --output <OUTPUT_PATH>` | Local transcript output path. Required for vtt and srt formats |
| `transcribe` | `--format <FORMAT>` | Transcript output format: text, json, vtt, or srt [default: text] |
| `transcribe, speak` | `--language <LANGUAGE>` | Optional model language hint, such as en |
| `transcribe` | `--timestamps` | Ask the runtime to return timestamp chunks when supported |
| `transcribe, speak` | `-H, --home <HOME>` | Optional Tentgent runtime home override |
| `speak` | `--text <TEXT>` | Text to synthesize |
| `speak` | `--text-file <PATH>` | UTF-8 text file to synthesize |
| `speak` | `-m, --model-ref <MODEL_REF>` | Stored Tentgent audio-speech model reference to run |
| `speak` | `-o, --output <OUTPUT_PATH>` | Local speech audio output path |
| `speak` | `--format <FORMAT>` | Speech output format. M6P supports wav [default: wav] |
| `speak` | `--voice <VOICE>` | Optional model voice or speaker hint |

## HTTP API

Start the [daemon](../daemon.md) and follow the [HTTP authentication and error rules](../api.md).

### Audio Transcription Jobs

Canonical audio transcription uses a workflow job:

```http
POST /v1/audio/transcriptions/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `file` | yes | file bytes | Audio bytes. The daemon does not receive or trust the client's local path. |
| `model_ref` | yes | text | Local `audio-transcription` model ref or unique alias. |
| `output_format` | no | text | `text`, `json`, `vtt`, or `srt`; defaults to `text`. |
| `language` | no | text | Use with multilingual checkpoints. Omit for English-only checkpoints. |
| `timestamps` | no | boolean text | `true`, `false`, `1`, `0`, `yes`, `no`, `on`, or `off`. |
| `output_filename` | no | text | File name only, not a path. |

`file` must appear exactly once. Audio transcription treats one request as one
logical audio input and one job. Send multiple audio files as multiple jobs, or
merge them before upload when a single transcript over a combined recording is
intended.

`vtt` and `srt` are subtitle formats. They require segment-level timestamps
from the selected backend; if the runtime cannot produce segment timings, the
job fails instead of writing untimed subtitles.

`curl` example:

```bash
curl -sS http://127.0.0.1:8790/v1/audio/transcriptions/job \
  -F model_ref=<audio-transcription-model-ref> \
  -F output_format=text \
  -F file=@/absolute/path/audio.mp3
```

In client code, `file` can be any byte array placed into the multipart file
part. `file=@/absolute/path/audio.mp3` is only curl shorthand for "read this
local file and send its bytes"; it is not a path-based API contract. The daemon
stores those received bytes in the job workspace and then passes the internal
workspace file path to the runtime worker.

Only one `file` part is accepted per request. Send multiple recordings as
multiple jobs, or merge them client-side when one combined transcript is the
desired output.

The upload body is transport-stream friendly: clients may stream the multipart
request body, and the daemon writes the file part to disk instead of treating
the client's local path as input. This is an I/O and memory boundary, not a
promise that the selected model performs realtime or partial-file inference.
The daemon-wide media upload cap applies to the `file` file part.

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "audio_transcription",
    "status": "queued",
    "target": {
      "section": "audio",
      "reference": "<model-ref>",
      "path": "<daemon-internal-workspace-input-path>"
    }
  }
}
```

Read status and result:

The result endpoint is
`GET /v1/audio/transcriptions/job/{job_id}/result` with optional `cursor` and
`max_chunks` query parameters.

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/audio/transcriptions/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o transcript.txt
```

Result route behavior:

| State | HTTP | Error code or response |
| --- | --- | --- |
| Job queued/running/intake | `409` | `result_pending` |
| Job failed | `409` | `job_failed` |
| Job interrupted | `409` | `job_interrupted` |
| Job canceled | `409` | `job_canceled` |
| Job succeeded but artifact missing | `404` | `result_not_found` |
| Result ready | `200` | Transcript bytes with `Content-Type`, `Content-Disposition`, `x-tentgent-next-cursor`, `x-tentgent-result-done`, and `x-tentgent-chunks-read`. |

Result reads are also transport-bounded: clients can read from `cursor` in
batches instead of requiring one full result read. Future large artifact routes
may stream response bodies or support range reads under workflow-owned routes;
they should not expose generic workspace or chunk internals.

Compatibility route:

```http
POST /v1/audio/transcriptions/jobs
GET  /v1/audio/transcriptions/jobs/{job_id}/result
```

The plural route is an undocumented alpha/debug compatibility path for trusted
local JSON path input. New clients should use the singular multipart route.

### Audio Speech

Canonical audio speech uses a workflow job:

```http
POST /v1/audio/speech/job
Content-Type: application/json
```

Request body:

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

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `model_ref` | yes | string | Local `audio-speech` model ref or unique alias. |
| `text` | yes | string | Non-empty UTF-8 text. The default limit is 64 KiB. |
| `output_format` | no | string | `wav` or `wave`; defaults to `wav`. |
| `output_filename` | no | string | File name only, not a path. Defaults to `speech.wav`. |
| `language` | no | string | Model-aware language hint. Unsupported values fail clearly. |
| `voice` | no | string | Model-aware voice or speaker hint. Unsupported values fail clearly. |

The route rejects unknown fields. `TENTGENT_AUDIO_SPEECH_MAX_TEXT_BYTES`
controls the maximum text byte length before daemon startup and defaults to
65536 bytes. M6P writes WAV only; `mp3`, realtime speech streaming,
speech-to-speech, SSML, and voice cloning are out of scope.

`curl` example:

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

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "audio_speech",
    "status": "queued",
    "target": {
      "section": "audio",
      "reference": "<model-ref>",
      "path": null
    }
  }
}
```

Read status and result:

The result endpoint is `GET /v1/audio/speech/job/{job_id}/result` with optional
`cursor` and `max_chunks` query parameters.

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/audio/speech/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o speech.wav
```

Result route behavior:

| State | HTTP | Error code or response |
| --- | --- | --- |
| Job queued/running/intake | `409` | `result_pending` |
| Job failed | `409` | `job_failed` |
| Job interrupted | `409` | `job_interrupted` |
| Job canceled | `409` | `job_canceled` |
| Job succeeded but artifact missing | `404` | `result_not_found` |
| Result ready | `200` | WAV bytes with `Content-Type`, `Content-Disposition`, `x-tentgent-next-cursor`, `x-tentgent-result-done`, and `x-tentgent-chunks-read`. |

Result reads are cursor-based like audio transcription. They are an artifact
download boundary, not realtime model streaming.

## Related Guides

[Model fixtures](../model-fixtures.md) · [Jobs and results](../jobs.md) · [Inference index](./README.md)
