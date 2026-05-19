# HTTP API Reference

This page summarizes the user-facing local HTTP API exposed by
`tentgent daemon`. Start the daemon before calling `/v1/*` routes:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
```

`GET /healthz` is always unauthenticated. `/v1/*` routes are protected when a
daemon token is configured; pass it as:

```bash
Authorization: Bearer $TENTGENT_DAEMON_TOKEN
```

Unless noted otherwise, request bodies are JSON and responses are JSON. Errors
use this shape:

```json
{
  "error": "bad_request",
  "message": "human-readable detail"
}
```

References such as `model_ref`, `adapter_ref`, `dataset_ref`, `server_ref`, and
`job_id` accept full refs where available; many routes also accept unique short
prefixes.

## Diagnostics

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/healthz` | Process liveness and service identity. |
| `GET` | `/v1/status` | Daemon status and runtime-home summary. |
| `GET` | `/v1/auth` | Local provider auth presence. Does not reveal secrets. |
| `GET` | `/v1/auth/{provider}` | Provider auth presence for `hf`, `openai`, `anthropic`, or `gemini`. |
| `GET` | `/v1/doctor` | Observational runtime and dependency report. |
| `GET` | `/v1/daemon/logs` | Daemon log metadata. |
| `GET` | `/v1/daemon/logs/stdout?tail_bytes=8192` | Daemon stdout tail. |
| `GET` | `/v1/daemon/logs/stderr?tail_bytes=8192` | Daemon stderr tail. |
| `POST` | `/v1/daemon/shutdown` | Ask the daemon process to shut down. |

## Chat

Native Tentgent chat:

```http
POST /v1/chat
Content-Type: application/json
```

```json
{
  "model_ref": "<chat-model-ref>",
  "adapter_ref": "<optional-adapter-ref>",
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 128,
  "temperature": 0.0,
  "stream": false
}
```

`messages[].role` supports `system`, `user`, and `assistant`. `stream=true`
returns Server-Sent Events.

Compatibility adapters route to the same chat execution path and are text-only:

| Method | Path | Request notes |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI-style `model`, `messages`, optional `adapter_ref`, `max_tokens`, `max_completion_tokens`, `temperature`, `stream`. |
| `POST` | `/v1/messages` | Claude-style `model`, `messages`, optional `system`, `adapter_ref`, `max_tokens`, `temperature`, `stream`. |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini-style `contents`, optional `systemInstruction`, `generationConfig`, `adapter_ref`. |
| `POST` | `/v1beta/models/{model}:streamGenerateContent?alt=sse` | Gemini-style streaming response. |

Tools, function calling, image/audio content, and non-text message parts are
rejected until the corresponding kernel features exist.

## Embeddings

```http
POST /v1/embeddings
Content-Type: application/json
```

```json
{
  "model_ref": "<embedding-model-ref>",
  "input": ["first text", "second text"]
}
```

`input` may be one string or an array of strings. The model must have
`embedding` capability metadata.

## Rerank

```http
POST /v1/rerank
Content-Type: application/json
```

```json
{
  "model_ref": "<rerank-model-ref>",
  "query": "refund policy",
  "documents": ["first candidate", "second candidate"],
  "top_n": 1
}
```

`documents` must be a non-empty string array. `top_n` is optional. The model
must have `rerank` capability metadata.

## Audio Transcription Jobs

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

The upload body is transport-stream friendly: clients may stream the multipart
request body, and the daemon writes the file part to disk instead of treating
the client's local path as input. This is an I/O and memory boundary, not a
promise that the selected model performs realtime or partial-file inference.

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

## Jobs

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/jobs` | List daemon-managed jobs. |
| `GET` | `/v1/jobs/{job_id}` | Inspect one job. |
| `POST` | `/v1/jobs/{job_id}/cancel` | Cancel an active job when supported. |
| `DELETE` | `/v1/jobs/{job_id}` | Delete a terminal job record and workspace. Active jobs return conflict. |

Jobs are used for detached model/adapter/dataset operations and media
workflows. The daemon manages job workspaces; public APIs do not expose
workspace chunks or spool routes.

## Models

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/models` | None. |
| `GET` | `/v1/models/{reference}` | None. |
| `DELETE` | `/v1/models/{reference}` | None. |
| `PATCH` | `/v1/models/{reference}` | `{"capability":"chat\|embedding\|rerank\|audio-transcription\|audio-speech\|vision-chat\|image-generation"}` |
| `POST` | `/v1/models/import` | `{"path":"/absolute/model-dir","capability":"optional-capability"}` |
| `POST` | `/v1/models/pull` | `{"repo_id":"org/model","revision":"optional","capability":"optional-capability"}` |
| `POST` | `/v1/models/import/jobs` | Same as `/v1/models/import`, returns a job. |
| `POST` | `/v1/models/pull/jobs` | Same as `/v1/models/pull`, returns a job. |

## Adapters

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/adapters` | None. |
| `GET` | `/v1/adapters/{reference}` | None. |
| `DELETE` | `/v1/adapters/{reference}` | None. |
| `POST` | `/v1/adapters/import` | `{"path":"/absolute/adapter-dir","base_model_ref":"optional-base-model"}` |
| `POST` | `/v1/adapters/pull` | `{"repo_id":"org/adapter","revision":"optional","base_model_ref":"optional-base-model"}` |
| `POST` | `/v1/adapters/import/jobs` | Same as `/v1/adapters/import`, returns a job. |
| `POST` | `/v1/adapters/pull/jobs` | Same as `/v1/adapters/pull`, returns a job. |
| `POST` | `/v1/adapters/{reference}/bind` | `{"base_model_ref":"<base-model-ref>"}` |

## Datasets

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/datasets` | None. |
| `GET` | `/v1/datasets/{reference}` | None. |
| `DELETE` | `/v1/datasets/{reference}` | None. |
| `POST` | `/v1/datasets/import` | `{"path":"/absolute/dataset-path"}` |
| `POST` | `/v1/datasets/import/jobs` | Same as `/v1/datasets/import`, returns a job. |
| `POST` | `/v1/datasets/validate` | `{"path":"optional-path","dataset_ref":"optional-ref"}` |
| `POST` | `/v1/datasets/template` | `{"task":"optional-task","language":"optional-language"}` |
| `POST` | `/v1/datasets/{reference}/export` | `{"output_path":"/absolute/output-path"}` |
| `POST` | `/v1/datasets/{reference}/diff` | `{"right_dataset_ref":"optional-ref","right_path":"optional-path"}` |
| `POST` | `/v1/datasets/synth/jobs` | Cloud dataset synthesis job; fields include `provider`, `model`, `output_path`, `brief`, `spec_content`, `spec_path`, split/count fields, `max_tokens`, `temperature`, `timeout_seconds`, and `retries`. |
| `POST` | `/v1/datasets/eval/jobs` | Cloud dataset evaluation job; fields include `provider`, `model`, `output_path`, `dataset_ref`, `input_content`, `input_format`, `input_path`, `split`, `max_records`, `criteria`, `max_tokens`, `temperature`, and `timeout_seconds`. |

## LoRA Training

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/train/lora/plans` | None. |
| `POST` | `/v1/train/lora/plans` | `{"model_ref":"...","dataset_ref":"...","name":"optional","backend":"optional","overrides":{...}}` |
| `POST` | `/v1/train/lora/plans/preview` | Same as create, but does not persist. |
| `GET` | `/v1/train/lora/plans/{reference}` | None. |
| `DELETE` | `/v1/train/lora/plans/{reference}` | None. |
| `GET` | `/v1/train/lora/plans/{reference}/runs` | None. |
| `POST` | `/v1/train/lora/plans/{reference}/runs` | Starts a training run job. |
| `GET` | `/v1/train/lora/runs` | None. |
| `GET` | `/v1/train/lora/runs/{reference}` | None. |
| `GET` | `/v1/train/lora/runs/{reference}/metrics?tail=100` | Metrics tail. |
| `GET` | `/v1/train/lora/runs/{reference}/logs?tail_bytes=8192` | Log tail metadata and content. |
| `GET` | `/v1/train/lora/runs/{reference}/logs/raw?tail_bytes=8192` | Raw log tail. |

`overrides` may include `max_seq_length`, `mask_prompt`, `rank`,
`learning_rate`, `batch_size`, `gradient_accumulation_steps`, `max_steps`,
`seed`, `mlx_num_layers`, `mlx_grad_checkpoint`, `peft_load_in_4bit`, and
`peft_load_in_8bit`.

## Managed Servers

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/servers` | None. |
| `POST` | `/v1/servers` | `{"runtime_ref":"<model-or-cloud-ref>","capability":"chat\|embedding\|rerank","host":"optional","port":8780,"lazy_load":true,"idle_seconds":60}` |
| `GET` | `/v1/servers/{reference}` | None. |
| `DELETE` | `/v1/servers/{reference}` | Removes a stopped server spec. |
| `POST` | `/v1/servers/{reference}/start` | `{"wait_ready":true,"timeout_seconds":30}` |
| `POST` | `/v1/servers/{reference}/stop` | None. |
| `GET` | `/v1/servers/{reference}/health` | Probe server process health. |
| `GET` | `/v1/servers/{reference}/logs` | Server log metadata. |
| `GET` | `/v1/servers/{reference}/logs/stdout?tail_bytes=8192` | Server stdout tail. |
| `GET` | `/v1/servers/{reference}/logs/stderr?tail_bytes=8192` | Server stderr tail. |

Direct model-server ports are separate from the daemon port. A chat server
exposes chat routes, an embedding server exposes `/v1/embeddings`, and a rerank
server exposes `/v1/rerank`. Unsupported endpoint families on that direct
server should return `404` or an endpoint-specific error.

## Sessions

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/sessions` | None. |
| `POST` | `/v1/sessions` | `{"title":"optional","default_server_ref":"optional","adapter_ref":"optional","tags":[],"messages":[]}` |
| `GET` | `/v1/sessions/{reference}` | None. |
| `PATCH` | `/v1/sessions/{reference}` | `{"title":"new-or-null","default_server_ref":"new-or-null","adapter_ref":"new-or-null","tags":["..."]}` |
| `DELETE` | `/v1/sessions/{reference}` | None. |
| `GET` | `/v1/sessions/{reference}/messages?tail=100` | Session transcript tail. |
| `POST` | `/v1/sessions/{reference}/messages` | `{"messages":[{"role":"user","content":"...","server_ref":"optional","adapter_ref":"optional","metadata":{}}],"compaction_server_ref":"optional"}` |
| `POST` | `/v1/sessions/{reference}/compact` | `{"server_ref":"optional","keep_recent_messages":49,"instructions":"optional"}` |

Session messages are text records. Multimodal chat transcript content is not
implemented yet.
