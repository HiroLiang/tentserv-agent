# Video Understanding

Prepare the [local-model runtime profile](../runtime.md#media-runtime-dependencies), a model with the matching capability, and any input files. See [file and HTTP rules](./README.md#file-and-http-media-rules) before choosing CLI or daemon execution.

## Examples And Common Operations

### Video Understanding

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

## Parameters

Use the installed command’s `-h` or `--help` for required arguments and version-specific options.

| Command | Option | Meaning |
| --- | --- | --- |
| `video understand` | `-m, --model-ref <MODEL_REF>` | Stored Tentgent video-understanding model reference to run |
| `video understand` | `-p, --prompt <TEXT>` | Prompt to ask about the video |
| `video understand` | `--system-prompt <TEXT>` | Optional system prompt |
| `video understand` | `-o, --output <OUTPUT_PATH>` | Local output path. Existing files are never overwritten |
| `video understand` | `--format <FORMAT>` | Output format intent: text, json, or md [default: text] |
| `video understand` | `--max-tokens <N>` | Optional max generated tokens |
| `video understand` | `--temperature <FLOAT>` | Optional sampling temperature |
| `video understand` | `--sample-fps <FPS>` | Frames per second to sample from the video. Default is 1.0 |
| `video understand` | `--max-frames <N>` | Maximum sampled frames. Default is 32 |
| `video understand` | `--max-frame-edge <PIXELS>` | Resize sampled frames so the largest edge is at most this many pixels. Default is 768 |
| `video understand` | `--clip-start-seconds <SECONDS>` | Optional clip start offset in seconds |
| `video understand` | `--clip-duration-seconds <SECONDS>` | Optional clip duration in seconds |
| `video understand` | `-H, --home <HOME>` | Optional Tentgent runtime home override |

## HTTP API

Start the [daemon](../daemon.md) and follow the [HTTP authentication and error rules](../api.md).

### Video Understanding Jobs

Canonical video understanding uses a workflow job:

```http
POST /v1/video/understanding/job
Content-Type: multipart/form-data
```

Multipart fields:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `file` | yes | file bytes | Video bytes. The daemon does not receive or trust the client's local path. |
| `model_ref` | yes | text | Local `video-understanding` model ref or unique alias. |
| `prompt` | yes | text | User prompt for the video. |
| `system_prompt` | no | text | Optional instruction prefix. |
| `output_format` | no | text | `text`, `json`, or `md`; defaults to `text`. |
| `output_filename` | no | text | File name only, not a path. Defaults to `video-understanding.<format>`. |
| `max_tokens` | no | integer text | Optional generation cap. |
| `temperature` | no | float text | Optional sampling temperature. |
| `sample_fps` | no | float text | Frame sampling rate. Defaults to 1.0; valid range is 0.1..4.0. |
| `max_frames` | no | integer text | Sampled frame cap. Defaults to 32; valid range is 1..128. |
| `max_frame_edge` | no | integer text | Resize sampled frames by largest edge. Defaults to 768; valid range is 128..1536. |
| `clip_start_seconds` | no | float text | Optional non-negative clip start offset. |
| `clip_duration_seconds` | no | float text | Optional positive clip duration. |

`file` must appear exactly once. Send multiple videos as multiple jobs, or
merge them client-side when one combined analysis is intended. The daemon stores
the uploaded bytes in the job workspace, samples bounded frames through the
Python local-model runtime, and then calls the selected
`video-understanding` model. This is not realtime video streaming.

The first runnable baseline samples frames using the local-model Python
runtime's OpenCV-backed decoder. Codec/container support depends on the
packaged OpenCV/FFmpeg build and OS platform. Missing Python decoder packages
and unsupported system codecs fail the job with runtime error details.

`curl` example:

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

Response:

```json
{
  "job": {
    "job_id": "job-...",
    "kind": "video_understanding",
    "status": "queued",
    "target": {
      "section": "video",
      "reference": "<model-ref>",
      "path": "<daemon-internal-workspace-input-path>"
    }
  }
}
```

Read status and result:

The result endpoint is
`GET /v1/video/understanding/job/{job_id}/result` with optional `cursor` and
`max_chunks` query parameters.

```bash
curl -sS http://127.0.0.1:8790/v1/jobs/<job-id>
curl -sS \
  'http://127.0.0.1:8790/v1/video/understanding/job/<job-id>/result?cursor=0&max_chunks=32' \
  -o video-understanding.txt
```

Result route behavior matches audio transcription: queued/running jobs return
`409 result_pending`; failed, interrupted, or canceled jobs return a terminal
conflict; ready results return bytes with `Content-Type`,
`Content-Disposition`, `x-tentgent-next-cursor`, `x-tentgent-result-done`, and
`x-tentgent-chunks-read`.

## Related Guides

[Model fixtures](../model-fixtures.md) · [Jobs and results](../jobs.md) · [Inference index](./README.md)
