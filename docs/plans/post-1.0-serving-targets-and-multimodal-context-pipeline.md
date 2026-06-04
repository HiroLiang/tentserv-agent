# Post-1.0 Serving Targets And Multimodal Context Pipeline

Status: deferred post-1.0 planning note.

This plan records a future direction that should influence current 1.0 design
decisions without becoming a `1.0.0` release blocker.

Tentgent should eventually support a single API target backed by multiple
capability-specific models. That target can route chat, embedding, rerank,
vision, audio, video, and image-generation work to the configured model for
each capability.

## Why This Is Post-1.0

The `1.0.0` line should first stabilize provider-compatible API contracts,
model support status, runtime profiles, verification gates, and clear
unsupported behavior.

Serving targets and automatic multimodal context assembly depend on those
foundations. They should not delay `1.0.0`, but current contracts should avoid
closing the door on them.

## Future Capability

A future serving target might be configured like this:

```yaml
target: local-assistant
chat: llama-3.1-8b
embedding: bge-m3
rerank: bge-reranker
vision: qwen2.5-vl
audio_transcription: whisper
video_understanding: video-llava
image_generation: flux
```

Callers would use one base URL or target name, while Tentgent chooses the
configured model for each requested capability.

## Multimodal Context Pipeline

For local requests that include files, images, audio, or video, Tentgent can
eventually pre-process attachments before sending the final prompt to the chat
model:

1. Accept a chat or native request with attached media.
2. Detect attachment type and route it to the configured capability model.
3. Convert model output into a bounded context artifact such as an image
   caption, transcript, scene summary, extracted text, or file summary.
4. Compose those artifacts into the chat context.
5. Dispatch the final text context to the configured chat model.

This makes a local cluster feel like one assistant endpoint while keeping each
model focused on the capability it can actually perform.

## Design Constraints To Preserve Before 1.0

- Keep native request intent types separate from provider-shaped payloads so
  future pipelines can assemble context without pretending the backend is still
  OpenAI, Claude, or Gemini shaped.
- Keep model capability metadata explicit enough to express `chat`,
  `embedding`, `rerank`, `vision`, `audio_transcription`,
  `video_understanding`, and `image_generation`.
- Keep unsupported multimodal fields rejected clearly when no pipeline exists.
- Keep runtime profile and resource gating extensible enough to reason about
  more than one model loaded for one serving target.
- Keep attachment handling separate from chat text parsing so future file and
  media processing can run before chat dispatch.
- Keep proof records tied to the model, backend, platform, and capability tuple,
  because a serving target is only as stable as its weakest configured
  capability.

## Non-Goals Before 1.0

- No automatic local multimodal context assembly.
- No multi-model serving target configuration surface.
- No promise that provider-compatible multimodal requests behave like local
  native multimodal pipelines.
- No implicit fallback to another model when a configured capability is missing.

## Likely Post-1.0 Slices

1. Define the serving target schema and validation rules.
2. Add a read-only target inspection command and API.
3. Start with native local routing across chat, embedding, and rerank.
4. Add explicit attachment records and bounded context artifacts.
5. Add vision/audio/video pre-processing into chat context.
6. Add resource gating for multi-model runtime plans.
7. Decide which provider-compatible routes can safely use the pipeline.
