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

A future serving target, also called a cluster target, should represent one
server-startable routing rule set. Callers use one base URL or target name, and
Tentgent routes each request to the configured model or provider for that
capability.

A draft cluster YAML might look like this:

```yaml
version: 1
target: local-assistant
server:
  host: 127.0.0.1
  port: 8790
auth:
  mode: auto
  secret_file: .env
  allow_openshell_broker: true
routes:
  chat:
    model: llama-3.1-8b
  embedding:
    model: bge-m3
  rerank:
    model: bge-reranker
  vision:
    model: qwen2.5-vl
    output: context
  audio_transcription:
    model: whisper
    output: context
  audio_speech:
    model: kokoro
  video_understanding:
    model: video-llava
    output: context
  image_generation:
    model: flux
provider_compat:
  openai: true
  anthropic: true
  gemini: true
```

The initial schema should stay declarative. It should not store provider
secrets or runtime proof blobs inline. A cluster is valid only when every
configured route can be resolved to a known model/provider capability and a
compatible runtime profile.

The `output: context` marker means the route is intended for pre-processing
incoming attachments before chat dispatch. For example, image, audio, video, or
file inputs can be routed to the matching capability model, converted into a
bounded context artifact, and then appended to the chat model input.

## Auth Source Strategy

As of `v0.8.0`, normal provider auth resolution already supports `auto`,
`keychain`, `file`, `env`, and `none` modes outside cluster targets. Cluster
work should reuse that foundation instead of introducing a second secret
resolver.

Cluster targets should support multiple auth source modes because they may run
inside normal local shells, restricted shells, and future OpenShell-managed
agent environments:

- `keychain`: read provider secrets only through Tentgent-managed Keychain or
  equivalent OS secret storage.
- `file`: read provider secrets from an explicitly configured local secret
  file.
- `env`: read process environment variables only.
- `none`: disable provider secret lookup for flows that intentionally rely on
  caller-supplied credentials or non-provider local routes.
- `openshell`: do not materialize provider secrets in Tentgent config; call
  through an OpenShell-managed provider or gateway so OpenShell supplies the
  credential boundary.
- `auto`: compose existing request, environment, file, and Keychain sources,
  then use an OpenShell broker/gateway only when that future boundary is
  explicitly configured.

OpenShell integration should be modeled as an auth boundary, not as a hidden
provider key fallback. If Tentgent is launched inside an OpenShell environment,
it should be able to use environment or provider metadata supplied by
OpenShell, or send provider-routed requests through the OpenShell boundary
without requiring Tentgent to persist the raw provider key.

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

## Cloud Rerank Direction

Provider-compatible rerank remains outside the `v1.0.0` stable promise.
OpenAI, Claude/Anthropic, and the Gemini Developer API do not expose a stable
rerank endpoint family that Tentgent can mirror as part of the current
compatibility contract.

Future cloud rerank support should be added as an explicit capability and
adapter family instead of being hidden behind chat, embeddings, or Gemini
`generateContent`. If Tentgent adopts a ranking API such as Google Vertex AI
Search ranking, that work should define its own provider identifier, request
and response contract, auth requirements, capability metadata, runtime profile,
and verification fixtures.

Native `/v1/rerank` remains the local Tentgent-shaped fallback until that
adapter exists. Provider-shaped rerank attempts should fail predictably rather
than being partially interpreted as native rerank requests.

## Tool-Use Orchestration

Provider-compatible tool use should also fit this serving-target model after
`1.0.0`. OpenAI tool calls, Claude `tool_use` / `tool_result` blocks, and
Gemini function calls should be translated into Tentgent-owned tool-call intent
types before any backend sees them.

The future target should decide whether a tool request is handled by an
external caller loop, a local capability model, or a configured Tentgent tool
adapter. Tool-call and tool-result messages should remain provider-shaped only
at the ingress and response edges; the internal pipeline should keep native
tool-call records so chat, retrieval, media parsing, and capability routing can
share one contract.

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
- Keep provider-shaped tool calls separate from native tool-call intent records
  so OpenAI, Claude, and Gemini tool loops can share one orchestration layer.
- Keep proof records tied to the model, backend, platform, and capability tuple,
  because a serving target is only as stable as its weakest configured
  capability.

## Non-Goals Before 1.0

- No automatic local multimodal context assembly.
- No multi-model serving target configuration surface.
- No provider-compatible tool-call orchestration.
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
7. Add native tool-call intent records and provider-shaped tool event mapping.
8. Decide which provider-compatible routes can safely use the pipeline.
