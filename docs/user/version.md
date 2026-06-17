# Version Notes

This document summarizes the current user-facing version. It is not a changelog yet.

## v0.7.0

`v0.7.0` is the support status release. It makes model support explicit and
inspectable without turning support status into a hard runtime gate yet.

The active `v0.8.0` runtime-profile track extends those diagnostics with
runtime profile, execution backend, and copyable next-action details in
`model inspect`, `server inspect`, and `doctor`. Long details stay in inspect
or doctor detail blocks rather than expanding compact list tables.

What changed:

- Added a built-in model support catalog for curated fixture models and major
  public model families.
- Added support status vocabulary and local proof semantics for model,
  capability, backend, and runtime tuples.
- Added compact support summaries to `tentgent model ls`.
- Added detailed per-capability support rows to `tentgent model inspect`.
- Added local bound-model support details to `tentgent server inspect`.
- Added model support warnings to `tentgent doctor`.
- Added coarse interactive `doctor` progress messages on stderr while keeping
  redirected stdout clean.
- Documented operator next actions for `verified`, `supported`, `failed`,
  `unsupported`, `unknown`, and `stale`.

Known limits:

- Support status is visible but does not hard-block existing runtime commands
  in this release.
- Automatic proof updates from every successful or failed model request remain
  future work.
- Runtime profile selection and stricter support-status gates are deferred to
  the `v0.8.0` runtime profile and gating track.

## v0.6.0

`v0.6.0` is the compatibility contract release. It keeps the CLI plus daemon
REST product surface and makes the documented provider-shaped API subset
explicit for OpenAI, Claude/Anthropic, and Gemini compatibility.

What changed:

- Added a user-facing provider compatibility matrix for daemon compatibility
  adapters, direct cloud provider servers, and local model-bound servers.
- Added copy-paste curl and SDK examples for currently supported
  provider-shaped routes.
- Stabilized unsupported provider API error codes for unsupported fields,
  content parts, operations, and provider/capability combinations.
- Added compatibility fixtures for OpenAI chat, embeddings, image generation,
  vision input, and audio chat behavior currently supported by Tentgent.
- Added compatibility fixtures for Claude messages, vision messages, and known
  unsupported Claude embedding/audio behavior.
- Added compatibility fixtures for Gemini generateContent, streaming,
  embeddings, image/audio understanding, and image generation.
- Added Gemini image generation routing through Gemini image models and Imagen
  fallback routing while keeping unsupported image fields on stable errors.
- Clarified which provider-compatible media and tool-use behavior remains
  intentionally unsupported or deferred to later roadmap work.

Known limits:

- This is partial provider compatibility, not full OpenAI, Claude, or Gemini API
  parity.
- Tools/function calling, broader multimodal local routing, provider-compatible
  rerank, and cluster-style serving targets remain future work.
- Runtime profile selection and hard support-status gates remain future work.

## v0.5.2

`v0.5.2` is a patch release for readable runtime diagnostics.

What changed:

- `tentgent runtime status` now renders wrapped field/value rows instead of a
  two-column box table, so long missing-module and path messages do not stretch
  or break narrow terminals.
- `tentgent runtime bootstrap --profile all` is accepted as an alias for
  `--profile full`.

## v0.5.1

`v0.5.1` is a patch release for package-manager and direct-installer Python
runtime bootstrap.

What changed:

- Release artifacts now package the Python workspace root with `uv.lock`
  alongside `python/tentgent-model-runtime/`.
- `tentgent runtime bootstrap` resolves the packaged model-runtime project at
  `share/tentgent/python/tentgent-model-runtime`, so `uv --frozen` can use the
  packaged workspace lockfile.
- Direct installer scripts copy the packaged workspace lockfile and bootstrap
  the managed Python environment from the model-runtime project member.

## v0.5.0

`v0.5.0` is the mature model-runtime server release. It keeps the CLI plus
daemon REST product surface from `v0.4.1`, and changes local model servers to
run through a Rust local-server proxy backed by the shared Python model runtime
daemon.

What changed:

- Local `tentgent server run <model>` starts a Rust local-server proxy instead
  of a dedicated permanent Python server process. The proxy forwards the
  original path, query, headers, and request body to the shared Python model
  runtime daemon after binding the selected model and capability.
- Local server requests now reuse the same model runtime daemon lifecycle as
  one-shot CLI and daemon runtime calls. The Python runtime can idle-shutdown
  normally and be started again on demand instead of staying alive because a
  server was created.
- Added cross-process model-runtime launch locking so CLI, Rust daemon, and
  local server proxy callers do not race into duplicate Python runtime spawns
  for the same bound model/capability.
- Added Rust cloud provider server runtimes for OpenAI, Claude, and Gemini
  compatible chat surfaces, including OpenAI-compatible
  `/v1/chat/completions` and SSE streaming wrappers where supported.
- Added OpenAI cloud image-generation routing and fixed `gpt-image-1` requests
  by omitting the legacy `response_format` field that the model rejects.
- Added local model capability mutation and proof commands/API so operators can
  add, remove, set, verify, and inspect model capability metadata from the CLI
  and daemon routes.
- Expanded the direct model-runtime server boundary for chat, embedding,
  rerank, audio, vision, image, video understanding, and LoRA tuning endpoint
  families.
- Updated server and runtime contracts to document the shared runtime daemon,
  local proxy, cloud runtime, lifecycle, and idle policy boundaries.

Known limits:

- Compatibility proof storage is still local metadata-file based. It now keeps
  tuple-aware support proofs plus a legacy latest-proof file, but broader shared
  support registries, strict stale-proof gating, and dynamic runtime routing
  remain future roadmap work.
- Media upload workflows still use daemon job/workspace routes for bounded
  upload, cleanup, and result-file serving. Direct local server media routes are
  model-bound and path-based, not generic upload tunnels.
- Homebrew formula update happens after the GitHub release workflow publishes
  macOS release archives and `checksums.txt`.

## v0.4.1

`v0.4.1` is the signed macOS release for the CLI plus daemon REST and M6 media
workflow surface. It removes the former terminal UI surface and the legacy Rust
`core` / `http` crates so runtime behavior flows through `tentgent-kernel`,
`tentgent-cli`, and `tentgent-daemon`.

What changed:

- Removed the former terminal UI command and package.
- Removed the legacy Rust HTTP daemon crate; `tentgent daemon` now launches the
  daemon host directly.
- Removed the legacy Rust core crate after dependency audit confirmed CLI and
  daemon paths use kernel use cases.
- Kept `runtime status` scoped to runtime initialization state and moved broad
  platform/backend/runtime-footprint diagnostics under `doctor`.
- Added daemon-native chat and OpenAI, Claude, and Gemini compatible adapters
  as text-only DTO/SSE translators over the existing kernel chat use cases.
- Added daemon-native `POST /v1/embeddings` for local safetensors embedding
  models and direct local model-server `/v1/embeddings` for `--capability
  embedding` server specs.
- Added daemon-native `POST /v1/rerank` for local safetensors rerank models and
  direct local model-bound server routes for local chat, embedding, rerank,
  audio, vision, video, and image server specs.
- Added one-shot CLI `tentgent embed` and `tentgent rerank` commands for local
  embedding and rerank smoke tests without starting the daemon.
- Added foreground CLI `tentgent transcribe` for local file-to-file audio
  transcription with `text`, `json`, `vtt`, and `srt` output formats. The
  command fails if the requested output file already exists and does not create
  daemon jobs.
- Added foreground CLI `tentgent vision chat` and daemon-native
  `POST /v1/vision/chat` for bounded single-image plus prompt requests against
  local `vision-chat` safetensors models.
- Added foreground CLI `tentgent image generate` and daemon-native
  `POST /v1/images/generations/job` for text-to-image Diffusers and Apple
  Silicon MFLUX `image-generation` models. Generated files are exposed through
  `/v1/images/generations/job/{job_id}/files` and
  `/v1/images/generations/job/{job_id}/files/{file_id}`.
- Added foreground CLI `tentgent image transform` and daemon-native
  `POST /v1/images/transforms/job` for one-input-image image-to-image jobs
  against local `image-generation` models. Transform result files are exposed
  through `/v1/images/transforms/job/{job_id}/files` and
  `/v1/images/transforms/job/{job_id}/files/{file_id}`.
- Added foreground CLI `tentgent image inpaint` and daemon-native
  `POST /v1/images/inpaint/job` for masked inpainting jobs against local
  `image-generation` models. Mask semantics are `white = repaint` and
  `black = keep`. Inpaint result files are exposed through
  `/v1/images/inpaint/job/{job_id}/files` and
  `/v1/images/inpaint/job/{job_id}/files/{file_id}`.
- Added foreground CLI `tentgent image control` and daemon-native
  `POST /v1/images/control/job` for typed controlled image generation against
  local `image-generation` models plus managed ControlNet-style adapters.
  M6O supports `control_kind = canny`; result files are exposed through
  `/v1/images/control/job/{job_id}/files` and
  `/v1/images/control/job/{job_id}/files/{file_id}`.
- Added endpoint-family gates so chat routes require `chat` models and
  embedding/rerank routes require matching model capabilities before runtime
  dispatch.
- Added the first media daemon workflow:
  `POST /v1/audio/transcriptions/job` accepts multipart audio file upload,
  creates a background transcription job for local `audio-transcription`
  safetensors ASR models, and
  `GET /v1/audio/transcriptions/job/{job_id}/result` reads transcript result
  bytes from the job workspace.
- Added foreground CLI `tentgent speak` and daemon-native
  `POST /v1/audio/speech/job` for text-to-speech jobs against local
  `audio-speech` Transformers models. M6P writes WAV artifacts only; result
  bytes are exposed through `GET /v1/audio/speech/job/{job_id}/result`.
- Added foreground CLI `tentgent video understand` and daemon-native
  `POST /v1/video/understanding/job` for batch video-to-text jobs against local
  `video-understanding` models. M6Q samples bounded frames from an uploaded or
  local video, runs the selected model, and exposes result bytes through
  `GET /v1/video/understanding/job/{job_id}/result`.
- Added `TENTGENT_VIDEO_UPLOAD_MAX_BYTES` as a separate daemon upload limit for
  video file parts, defaulting to 512 MiB. Audio and image multipart uploads
  continue to use `TENTGENT_MEDIA_UPLOAD_MAX_BYTES`, defaulting to 20 MiB.
- Added the M7 GitHub Actions release path for macOS Developer ID signing and
  Apple notarization. macOS package jobs read Apple credentials from the
  `apple-developer` environment, sign the CLI with hardened runtime and a
  timestamp, submit a temporary zip payload for notarization, and keep the
  public macOS release archives as `.tar.gz`.

Known limits:

- Tools/function calling and audio content are rejected by compatible chat
  adapters until kernel tool-call and multimodal chat support exists. OpenAI,
  Claude, and Gemini compatible image payloads are still rejected; use the
  native `/v1/vision/chat` endpoint for single-image local vision requests.
- Audio transcription is batch file transcription. Multipart upload/result
  reads are transport-stream-friendly memory boundaries, not realtime model
  streaming. Live dictation and live translation remain future work.
- Audio speech is batch text-to-WAV synthesis. Realtime speech streaming,
  speech-to-speech, voice cloning, SSML, and MP3 output remain future work.
  Direct Python model-runtime MLX text-to-speech is available through
  `mlx-audio`; Rust daemon job routing still needs to be wired to that runtime.
- Video understanding is batch frame-sampled analysis, not realtime video
  streaming. The first runnable baseline uses the local-model Python runtime's
  OpenCV-backed decoder; codec/container support depends on the packaged
  OpenCV/FFmpeg build and OS platform. Direct Python model-runtime MLX video
  understanding is experimental and restricted to allow-listed `mlx-vlm`
  video-capable model types.
- Image generation supports text-to-image, one-input-image transform, one-mask
  inpainting, one managed image LoRA adapter, and one typed ControlNet-style
  control image workflow. Generic reference-image composition, multi-control
  inputs, automatic canny/depth/pose preprocessing, compatible OpenAI image
  APIs, direct image model server routes, image LoRA training, and broader
  compatibility proof storage are future slices.
- MLX acceleration is implemented for chat, LoRA training, native
  `vision-chat`, native `audio-transcription`, and MFLUX-backed
  `image-generation` on Apple Silicon. Diffusers remains the small
  cross-platform image baseline.
- Rerank scores are raw backend scores; compare them within one request/model
  family rather than across unrelated models.
- `v0.4.1` is the first release intended to verify the signed and notarized
  macOS GitHub Actions path.

## v0.3.4-alpha.2

`v0.3.4-alpha.2` is a Linux x86_64 preview release. It is not the stable
`latest` release yet, but it is the first release whose Linux tarball was
smoke-tested through install, base runtime bootstrap, and `doctor`.

What changed:

- Added a Linux x86_64 GitHub Release archive:
  `tentgent-0.3.4-alpha.2-x86_64-unknown-linux-gnu.tar.gz`.
- Verified the Unix installer on `ubuntu:24.04` / `linux/amd64` with
  `curl | bash`.
- Split managed Python runtime bootstrap into dependency profiles:
  `base`, `local-model`, `training`, and `full`.
- Made `base` the default runtime bootstrap profile so Linux can prepare the
  managed Python runtime without build tools or heavyweight ML packages.
- Verified the base runtime installs 29 Python distributions, uses Python
  `3.13.13`, uses pinned uv `0.11.7`, and passes `tentgent doctor`.

Known limits:

- Use the explicit `v0.3.4-alpha.2` release URL for Linux testing. The stable
  `latest` release still tracks the 0.3.x stable line and does not advertise
  Linux support.
- Linux support currently means x86_64 release tarball install plus default
  base runtime bootstrap. Local-model, training, GPU, Linuxbrew, `.deb`, and
  `.rpm` support are not claimed yet.
- Backend warnings for MLX, Transformers/PEFT, and llama-cpp are expected after
  a base-profile bootstrap because those heavier dependencies are opt-in. After
  `tentgent runtime bootstrap --profile full`, `tentgent doctor` should verify
  those backend modules by import; embedding and rerank probes remain future
  work and may still report unknown.

## v0.3.3

`v0.3.3` is a Homebrew maintenance release for the stable 0.3.x line. It keeps
the runtime and CLI feature surface from `v0.3.2` while making future tap
updates repeatable.

What changed:

- Added `scripts/update-homebrew-formula.sh` so maintainers can update the
  project Homebrew tap from GitHub Release `checksums.txt` without hand-copying
  macOS artifact URLs or SHA-256 values.
- Added fixture tests for formula update, dry-run, missing checksum, malformed
  tag, and prerelease rejection behavior.
- Documented the release-to-tap update workflow in the developer guide.

Known limits:

- The tap updater is edit-only. It does not run `brew`, commit, or push the tap
  repository automatically.
- Homebrew installs still require explicit `tentgent runtime bootstrap` before
  local Python runtime use.
- Developer ID signing and notarization are still deferred.

## v0.3.2

`v0.3.2` adds the package-manager friendly managed Python runtime setup entry
point for the stable 0.3.x line.

What changed:

- Added `tentgent runtime bootstrap` so Homebrew/manual package installs can
  prepare the managed Python runtime without calling packaged shell scripts
  directly.
- Added `tentgent runtime bootstrap --print-plan` for path inspection without
  syncing.
- Updated installed-runtime hints to point at the public CLI bootstrap command.

Known limits:

- Homebrew install stays lightweight and does not run Python bootstrap during
  formula installation.
- `--dry-run` asks `uv` to plan the sync and may still resolve pinned bootstrap
  tooling/cache; use `--print-plan` for non-mutating inspection.

## v0.3.1

`v0.3.1` is a macOS installer hotfix for the stable 0.3.x line. It keeps the
`v0.3.0` feature surface and improves first-run reliability for downloaded
release binaries.

What changed:

- macOS release packaging now ad-hoc signs the `tentgent` binary before it is
  archived.
- macOS installer runs a best-effort quarantine cleanup and ad-hoc re-sign after
  copying the binary into the install prefix.
- This reduces `zsh: killed` / Gatekeeper-style first-run failures for
  non-notarized release artifacts.

Known limits:

- Developer ID signing and notarization are still planned for a later release
  engineering slice.
- Users who already installed `v0.3.0` and hit a killed binary can either
  install `v0.3.1` or manually clear quarantine and ad-hoc sign the installed
  binary.

## v0.3.0

`v0.3.0` was the stable 0.3.x baseline for the former terminal UI alpha line
and the first release candidate for Homebrew tap distribution. The current
`v0.7.0` surface is CLI plus daemon REST only.

Added:

- Request-scoped session context summaries so `max_session_messages` is a prior
  context budget instead of a raw-tail-only selector.
- Rolling persisted session summaries so long sessions periodically preserve
  old context as one durable summary plus recent raw messages.
- Clear daemon-managed session chat boundaries; direct model-server chat is
  stateless and rejects session-only fields.
- Stale daemon runtime-home diagnostics that do not recreate missing runtime
  directories during observational checks.
- Release workflow safeguards so prerelease tags publish as GitHub prereleases
  and do not become latest stable releases.
- Human-facing size display in CLI tables and runtime footprint visibility
  for the managed Python environment and bootstrap caches.
- Historical terminal UI chat transcript rendering with natural wrapping and
  transcript scrolling.
- Stable install examples for `v0.3.0` ahead of the Homebrew tap formula slice.

Known limits:

- The terminal UI alpha line was removed in `v0.4.1`; no redesign track
  remains active.
- Homebrew tap distribution is planned next; `v0.3.0` prepares the stable tag
  and release assets that the formula will point at.
- macOS signing and notarization are still deferred.

## v0.3.0-alpha.2

`v0.3.0-alpha.2` was a bugfix preview release for the former terminal UI alpha
line. The current `v0.7.0` surface is CLI plus daemon REST only.

Added:

- Request-scoped session context summaries so `max_session_messages` is a prior
  context budget instead of a raw-tail-only selector.
- Rolling persisted session summaries so long sessions periodically preserve
  old context as one durable summary plus recent raw messages.
- Clear daemon-managed session chat boundaries; direct model-server chat is
  stateless and rejects session-only fields.
- Stale daemon runtime-home diagnostics that do not recreate missing runtime
  directories during observational checks.
- Release workflow safeguards so prerelease tags publish as GitHub prereleases
  and do not become latest stable releases.
- Human-facing size display in CLI tables and runtime footprint visibility
  for the managed Python environment and bootstrap caches.

Known limits:

- This terminal UI alpha line was removed in `v0.4.1`.
- `latest` installers may still be managed separately from prerelease adoption;
  use the explicit `v0.3.0-alpha.2` release URL when testing this preview.
- macOS signing and notarization are still deferred.

## v0.3.0-alpha.1

`v0.3.0-alpha.1` was a terminal UI preview release. It is kept here for
historical context; the current `v0.7.0` surface is CLI plus daemon REST
only.

Added:

- Terminal UI operator workflows for chat, sessions, jobs, resources, stores,
  servers, and LoRA training surfaces.
- Background daemon job records and terminal UI progress surfaces for long-running
  store/dataset actions without changing existing synchronous API behavior.
- Picker-first server and LoRA plan creation flows with review/preview pages.
- Guarded terminal UI actions for model, adapter, dataset, server, training, and session
  management through existing daemon routes.
- Compact session/server/adapter ref display in dense terminal UI and CLI session lists.
- More reliable detached local server startup, including bind preflight and
  early health/process observation.

Known limits:

- This terminal UI alpha line was removed in `v0.4.1`.
- `latest` installers may still be managed separately from prerelease adoption;
  use the explicit `v0.3.0-alpha.1` release URL when testing this preview.
- Server, training, store, and session mutations remain guarded through CLI and
  daemon REST paths.
- macOS signing and notarization are still deferred.

## v0.2.0

`v0.2.0` expands the local HTTP daemon from a chat surface into a programmatic
peer for the main CLI workflows.

Added:

- HTTP store parity for model, adapter, dataset, and stopped server inspect,
  import/pull, bind, and remove workflows.
- HTTP dataset tooling for validation, templates, provider-backed synthesis,
  provider-backed evaluation, export, and diff.
- HTTP LoRA plan and run APIs, including durable run records, metrics, and raw
  log inspection.
- HTTP auth status, observational doctor diagnostics, daemon logs, server logs,
  and token-gated daemon shutdown.
- HTTP and CLI session mutation, session-aware chat, OpenAI-compatible
  `session_ref` extensions, and destructive bounded session compaction.
- Server specs expose endpoint capability and reject non-chat models on chat
  server and chat route paths.
- Terminal UI operator console with status/settings screens, daemon discovery,
  explicit daemon start, non-secret config, guarded local Keychain setup, and
  read-only navigators for stores, servers, sessions, and LoRA training state.

Known limits:

- Provider key set/remove remains local-only through the CLI Keychain setup. No
  daemon HTTP secret mutation route exists.
- `doctor --fix` remains CLI-only; HTTP doctor is observational.
- `daemon start` is the primary background entry point for the HTTP daemon;
  foreground `daemon run` remains available for debugging.
- Cloud provider servers do not support request-time local adapters.
- Generated dataset splits are not deduplicated against each other yet.
- macOS signing and notarization are deferred to a later slice.

## v0.1.4

`v0.1.4` adds HTTP chat streaming through Server-Sent Events.

Added:

- `/v1/chat` supports `stream=true` with SSE `delta`, `done`, and `error` events.
- Local base-model servers can stream backend output incrementally.
- Compatible local adapters can stream through the same `adapter_ref` request shape as non-streaming chat.
- OpenAI, Anthropic, and Gemini cloud provider servers return the same Tentgent
  SSE response shape; the current Rust worker may coalesce provider output into
  one delta when a provider streaming adapter is not used.
- Streaming request validation preserves JSON errors before SSE headers when adapter, auth, model, or request preflight fails.

Known limits:

- Cloud provider servers do not support request-time local adapters.
- Streaming chunks follow backend or provider tokenization and may not align to full words.
- Generated dataset splits are not deduplicated against each other yet.

## v0.1.3

`v0.1.3` hardens provider-assisted dataset synthesis for tuning-data workflows.

Added:

- `dataset synth` can generate `train.jsonl`, `valid.jsonl`, `test.jsonl`, and `eval_cases.jsonl` in one package with split-specific count options.
- `dataset synth --retries` / `-r` retries each split independently after invalid provider JSON, schema mismatches, or transient provider errors.
- Multi-split synthesis writes successful split files immediately, so later split failures preserve earlier work.
- Provider generation prompts now include split-specific JSONL shape examples and stronger `tentgent.chat.v1` rules.
- Provider output parsing can repair a narrow class of extra-brace JSON mistakes around tool-result records and reports the repair as a warning.
- Synthesis failures write split-scoped debug artifacts under `_debug/<split>/`.

Known limits:

- Generated train, valid, and test splits are not deduplicated against each other yet.
- Dataset synthesis quality still depends on provider output and should be validated before import or training.
- Cloud provider servers do not support request-time local adapters.

## v0.1.2

`v0.1.2` adds cloud provider server routing and provider-assisted dataset workflows.

Added:

- OpenAI, Anthropic, and Gemini keys can be verified through `auth status`.
- `server run openai:<model>`, `server run claude:<model>`, and `server run gemini:<model>` can expose provider chat through the local `/v1/chat` surface.
- `dataset validate`, `dataset template`, `dataset synth`, and `dataset eval` help produce and review `tentgent.chat.v1` tuning data.
- server JSON responses preserve UTF-8 text for direct curl readability.

Known limits:

- Cloud provider servers do not support request-time LoRA adapters.
- Dataset synthesis quality still depends on provider output and should be validated before import or training.

## v0.1.1

`v0.1.1` is the first installable MVP target with macOS and Windows release artifacts.

Included:

- provider auth key management for Hugging Face, OpenAI, Anthropic, and Gemini
- content-addressed model store with local import and Hugging Face pull
- content-addressed adapter store with local import, Hugging Face pull, and train-run import
- content-addressed dataset store with import, export, diff, remove, and canonical chat schema support
- one-shot local chat for MLX, PEFT safetensors, and llama-cpp GGUF paths
- local HTTP chat server with server registry and process lifecycle commands
- managed LoRA train plans
- runnable MLX LoRA training loop
- runnable PEFT safetensors LoRA training loop
- installer-managed Python runtime bootstrap for direct installs and `tentgent runtime bootstrap` for package-manager installs
- Windows x86_64 `.zip` artifact and PowerShell installer

Known limits:

- macOS and Windows x86_64 are the first packaged install targets
- MLX requires Apple Silicon macOS
- MLX image generation is available for MFLUX Flux-family MLX models on Apple
  Silicon macOS; Diffusers remains the small cross-platform image baseline
- Windows runtime support is PEFT/safetensors-first; MLX is disabled
- HTTP chat is currently non-streaming
- `llama-cpp` external adapter execution is not implemented in this MVP
- macOS signing and notarization are deferred to a later slice

## Upgrade Expectations

Patch releases should preserve all user runtime data under `TENTGENT_HOME`.

If a future release needs a runtime metadata migration, `tentgent doctor` should detect that state and print the required next step before destructive changes are made.
