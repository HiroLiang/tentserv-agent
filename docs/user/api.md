# HTTP API Index

This page defines common HTTP conventions and routes you to feature request,
response, and curl examples. It describes the API exposed by `tentgent daemon`. Start the daemon before calling `/v1/*` routes:

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

Guarded model, capability, adapter, dataset, train-plan, Cluster, and server
mutations may return HTTP `409` with an additive `blockers` array:

```json
{
  "error": "cluster_in_use",
  "message": "cluster cannot be removed while referenced",
  "blockers": [
    {
      "kind": "server-spec",
      "code": "cluster-in-use",
      "reference": "<server-ref>",
      "reason": "server spec targets this cluster",
      "resource_ref": "<cluster-ref>",
      "operation": "delete-cluster",
      "next_actions": ["tentgent server rm <server-ref>"]
    }
  ]
}
```

Top-level guard codes include `model_in_use`, `capability_in_use`,
`adapter_in_use`, `dataset_in_use`, `train_plan_in_use`, `cluster_in_use`, and
`server_in_use`. Short transition contention returns `resource-busy`; state
that repeatedly changes during guarded validation returns
`resource-state-unstable`. Both are retryable HTTP `409` responses. The exact
fields and ordering rules are defined by
[resource-blockers.md](../contracts/resource-blockers.md).

Multipart audio/image endpoints use one daemon-wide upload cap for received
file bytes:

- The default is 20 MiB.
- Operators can adjust it with `TENTGENT_MEDIA_UPLOAD_MAX_BYTES` before
  starting the daemon.
- The cap applies to multipart file parts such as `image` on `/v1/vision/chat`
  and `file` on `/v1/audio/transcriptions/job`. JSON-only routes such as
  `/v1/audio/speech/job` use their own request limits.
- Video understanding uses a separate cap because video files are commonly much
  larger. `TENTGENT_VIDEO_UPLOAD_MAX_BYTES` defaults to 512 MiB and applies to
  `file` on `/v1/video/understanding/job`.
- The cap is an HTTP intake guard, not a model context limit. Model-specific
  image size, audio/video duration, token, or memory failures still come from
  the selected runtime.
- When an uploaded audio/image file part exceeds the cap, the daemon returns
  HTTP `413` with `upload_too_large`. When a video file exceeds the video cap,
  the daemon returns `video_upload_too_large`.

Example:

```json
{
  "error": "upload_too_large",
  "message": "`image` upload exceeds the daemon media upload limit of 20971520 bytes; set TENTGENT_MEDIA_UPLOAD_MAX_BYTES to adjust this limit"
}
```

Video example:

```json
{
  "error": "video_upload_too_large",
  "message": "`file` upload exceeds the daemon video upload limit of 536870912 bytes; set TENTGENT_VIDEO_UPLOAD_MAX_BYTES to adjust this limit"
}
```

References such as `model_ref`, `adapter_ref`, `dataset_ref`, `server_ref`, and
`job_id` accept full refs where available; many routes also accept unique short
prefixes.

For the `v1.0.0` stable, experimental, internal, and deprecated surface
classification, see
[api-surface-stability.md](../contracts/api-surface-stability.md). The linked feature guides describe callable HTTP routes; the stability tier and behavior
boundary for each surface is defined by that contract.

Feature pages distinguish native daemon requests from [model-bound servers](./servers.md) and [provider-compatible payloads](./providers/README.md). A daemon token does not configure authentication for separate direct server ports.

## Diagnostics

[Diagnostics routes, fields, and examples](./daemon.md#http-api).

Also see [provider auth status](./auth.md#http-api) and [runtime diagnostics](./maintenance.md#http-api).

## Chat

[Chat routes, fields, and examples](./inference/chat.md#http-api).

## Vision Chat

[Vision Chat routes, fields, and examples](./inference/vision.md#http-api).

## Video Understanding Jobs

[Video Understanding Jobs routes, fields, and examples](./inference/video.md#http-api).

## Image Generation Jobs

[Image Generation Jobs routes, fields, and examples](./inference/images/generate.md#http-api).

## Image Transform Jobs

[Image Transform Jobs routes, fields, and examples](./inference/images/edit.md#image-transform-jobs).

## Image Inpaint Jobs

[Image Inpaint Jobs routes, fields, and examples](./inference/images/edit.md#image-inpaint-jobs).

## Image Control Jobs

[Image Control Jobs routes, fields, and examples](./inference/images/edit.md#image-control-jobs).

## Embeddings

[Embeddings routes, fields, and examples](./inference/embedding-rerank.md#embeddings).

## Rerank

[Rerank routes, fields, and examples](./inference/embedding-rerank.md#rerank).

## Audio Transcription Jobs

[Audio Transcription Jobs routes, fields, and examples](./inference/audio.md#audio-transcription-jobs).

## Audio Speech

[Audio Speech routes, fields, and examples](./inference/audio.md#audio-speech-1).

## Jobs

[Jobs routes, fields, and examples](./jobs.md#http-api).

## Clusters

[Clusters routes, fields, and examples](./clusters.md#http-api).

## Models

[Models routes, fields, and examples](./models.md#http-api).

## Adapters

[Adapters routes, fields, and examples](./adapters.md#http-api).

## Datasets

[Datasets routes, fields, and examples](./datasets.md#http-routes).

## LoRA Training

[LoRA Training routes, fields, and examples](./training-lora.md#http-routes).

<a id="servers"></a>

## Managed Servers

[Managed Servers routes, fields, and examples](./servers.md#http-api).

## Sessions

[Sessions routes, fields, and examples](./sessions.md#http-api).
