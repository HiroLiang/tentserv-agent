# Cloud Runtime Test Tracker

Archived tracker for validating the Rust cloud provider runtime paths after
commit `6fe5bcc` (`Add Rust cloud provider runtime paths`). All planned blocks
in this tracker passed for OpenAI and this file is kept as implementation
history.

## Scope

- Rust cloud provider client for OpenAI, Anthropic, and Gemini.
- Rust direct cloud server worker launched by `tentgent server run <provider>:<model>`.
- Rust daemon cloud chat, embedding, and image routes.
- Provider-backed dataset synth/eval through the Rust cloud client.
- Keychain/env provider-secret resolution for cloud launch and daemon requests.

## Test Blocks

### Block 1: Static And No-Network Smoke

Status: passed.

Commands run:

```bash
cargo check -p tentgent-kernel
cargo check -p tentgent-daemon
cargo check -p tentgent-cli
cargo run -p tentgent-cli -- __cloud-server-runtime --help
env OPENAI_API_KEY=sk-test cargo run -p tentgent-cli -- __cloud-server-runtime \
  --server-ref smoke \
  --provider openai \
  --provider-model gpt-test \
  --host 127.0.0.1 \
  --port 8899 \
  --home /private/tmp/tentgent-cloud-smoke \
  --lazy-load \
  --idle-seconds 30
curl -sS http://127.0.0.1:8899/healthz
```

Observed result:

- All three `cargo check` commands passed.
- Hidden worker help parsed successfully and included `--home`,
  `--lazy-load`, and `--idle-seconds`.
- `/healthz` returned `runtime_kind: cloud`, `provider: openai`,
  `model: gpt-test`, `server_ref: smoke`, and the expected `runtime_home`.
- Test worker was stopped and port `8899` had no listener afterward.
- Worktree was clean after the block.

### Block 2: Cloud Server Lifecycle

Status: passed for OpenAI.

Goal:

- Start a provider server through the public CLI path.
- Verify `server ps`, `server inspect`, health probing, recorded `bound_port`,
  and `server stop`.

Suggested first command:

```bash
tentgent server run openai:<model> \
  --capability chat \
  --host 127.0.0.1 \
  --port 8898 \
  --home /private/tmp/tentgent-cloud-lifecycle \
  --detach
```

Notes:

- This path validates provider auth before launch, so use a real env or
  Keychain credential.
- Gemini is intentionally deferred unless requested.

Commands run:

```bash
cargo run -p tentgent-cli -- server run openai:gpt-4.1-mini \
  --capability chat \
  --host 127.0.0.1 \
  --port 8898 \
  --home /private/tmp/tentgent-cloud-lifecycle \
  --detach

target/debug/tentgent server ps --home /private/tmp/tentgent-cloud-lifecycle
target/debug/tentgent server inspect 38bc18d72d77 --home /private/tmp/tentgent-cloud-lifecycle
curl -sS http://127.0.0.1:8898/healthz
target/debug/tentgent server stop 38bc18d72d77 --home /private/tmp/tentgent-cloud-lifecycle
```

Observed result:

- Public `server run` created a cloud server spec for OpenAI
  `gpt-4.1-mini`.
- OpenAI auth validation succeeded from Keychain.
- Background launch succeeded and recorded `pid: 86272` with
  `bound_port: 8898`.
- `server ps` listed the running cloud server.
- `server inspect` showed the expected provider/model, runtime kind, host,
  requested port, and bound port.
- `/healthz` returned `runtime_kind: cloud`, the full `server_ref`, and
  `runtime_home: /private/tmp/tentgent-cloud-lifecycle`.
- `server stop` cleaned process metadata; afterward `server ps` was empty,
  `server inspect` reported stopped, and port `8898` had no listener.
- In the Codex sandbox, the first `server stop` could not send TERM; rerunning
  the same lifecycle stop command with escalation succeeded.

### Block 3: Direct Cloud Server Endpoints

Status: passed for OpenAI after image request fix.

Goal:

- Against a running direct cloud server, validate:
  - `POST /v1/chat`
  - `POST /v1/chat/completions`
  - `POST /v1/embeddings`
  - `POST /v1/images/generations`
  - `stream=true` SSE shape

Provider focus:

- OpenAI first.
- Anthropic chat and base64 image block next.
- Gemini deferred unless requested.

Commands run:

```bash
export TG_HOME=/private/tmp/tentgent-cloud-endpoints
export CHAT_PORT=8898
export EMBED_PORT=8897
export IMAGE_PORT=8896

target/debug/tentgent server ps --home "$TG_HOME"
curl -sS "http://127.0.0.1:$CHAT_PORT/healthz"
curl -sS "http://127.0.0.1:$CHAT_PORT/v1/chat" ...
curl -sS "http://127.0.0.1:$CHAT_PORT/v1/chat/completions" ...
curl -N "http://127.0.0.1:$CHAT_PORT/v1/chat/completions" ...
curl -sS "http://127.0.0.1:$CHAT_PORT/v1/chat/completions" ... # image input
cargo run -p tentgent-cli -- server run openai:text-embedding-3-small \
  --capability embedding --host 127.0.0.1 --port "$EMBED_PORT" \
  --home "$TG_HOME" --detach
curl -sS "http://127.0.0.1:$EMBED_PORT/v1/embeddings" ...
cargo run -p tentgent-cli -- server run openai:gpt-image-1 \
  --capability image-generation --host 127.0.0.1 --port "$IMAGE_PORT" \
  --home "$TG_HOME" --detach
curl -sS "http://127.0.0.1:$IMAGE_PORT/v1/images/generations" ...
```

Observed result:

- Direct OpenAI chat health returned the expected cloud runtime metadata.
- Native `/v1/chat` returned the requested exact text.
- OpenAI-compatible `/v1/chat/completions` returned OpenAI-shaped
  non-streaming output.
- `stream=true` returned Tentgent SSE events with one `delta` and one `done`.
- OpenAI-compatible chat with `image_url` content succeeded against
  `gpt-4.1-mini`.
- Direct embedding server for `text-embedding-3-small` launched through
  `server run` and returned two embedding vectors.
- Initial direct image generation failed because Tentgent sent
  `response_format: b64_json` to `gpt-image-1`, which OpenAI rejected with
  `Unknown parameter: 'response_format'`.
- Fixed the OpenAI image request builder to omit `response_format` for
  `gpt-image-*` models while preserving `b64_json` for legacy image models.
- After rebuilding, direct `gpt-image-1` generation returned HTTP 200 with one
  `b64_json` image payload.
- All direct cloud servers were stopped after testing.

### Block 4: Rust Daemon Cloud Routes

Status: passed for OpenAI.

Goal:

- Start the Rust daemon.
- Validate daemon cloud routes:
  - `/v1/chat/completions` cloud fallback
  - `/v1/embeddings` cloud fallback
  - `/v1/images/generations`

Notes:

- Confirm daemon auth headers if `TENTGENT_DAEMON_TOKEN` is set.
- Confirm local-model routes still remain local-first.

Commands run:

```bash
target/debug/tentgent daemon start \
  --home /private/tmp/tentgent-cloud-daemon \
  --host 127.0.0.1 \
  --port 8794

curl -sS http://127.0.0.1:8794/healthz
curl -sS http://127.0.0.1:8794/v1/status
curl -sS http://127.0.0.1:8794/v1/chat/completions ...
curl -sS http://127.0.0.1:8794/v1/embeddings ...
curl -sS http://127.0.0.1:8794/v1/images/generations ...
curl -sS -N http://127.0.0.1:8794/v1/chat/completions ... # stream=true
target/debug/tentgent daemon stop --home /private/tmp/tentgent-cloud-daemon
```

Observed result:

- Daemon launched on `127.0.0.1:8794` with runtime home
  `/private/tmp/tentgent-cloud-daemon`.
- `/healthz` returned `status: ok`.
- `/v1/status` returned `token_enabled: false`, the expected host/port, pid,
  runtime home, and no warnings.
- `/v1/chat/completions` with model `gpt-4.1-mini` returned HTTP 200 and
  OpenAI-shaped content `daemon-cloud-chat-ok`.
- `/v1/embeddings` with model `text-embedding-3-small` returned HTTP 200,
  two embedding records, and both vectors had dimension `1536`.
- `/v1/images/generations` with provider `openai` and model `gpt-image-1`
  returned HTTP 200 with one inline `b64_json` image payload. This confirms the
  `gpt-image-*` `response_format` fix applies to daemon cloud routes too.
- `/v1/chat/completions` with `stream: true` returned OpenAI chunk-shaped SSE
  events and ended with `data: [DONE]`.
- No auth header was required because `TENTGENT_DAEMON_TOKEN` was unset and the
  daemon was bound to loopback.
- Local-first behavior was confirmed by reading the daemon chat preparation
  path: compatible cloud chat routes resolve a local model selector before
  falling back to the provider. It was not externally exercised in this block
  because the temporary runtime home did not contain a local model fixture.
- In the Codex sandbox, the first `daemon stop` could not send TERM; rerunning
  the same stop command with escalation succeeded. Port `8794` had no listener
  afterward.

### Block 5: Dataset Synth/Eval

Status: passed for OpenAI.

Goal:

- Run provider-backed dataset synthesis with a small count.
- Import or validate generated split files if useful.
- Run dataset eval and verify `prompt.md` and `report.json`.

Provider focus:

- OpenAI first.
- Anthropic second if chat quality/eval path needs coverage.
- Gemini deferred unless requested.

Commands run:

```bash
target/debug/tentgent dataset synth \
  -p openai \
  -m gpt-4.1-mini \
  -o /private/tmp/tentgent-dataset-synth-openai \
  -b "Generate two concise English customer-support chat records about billing questions. Keep each assistant answer short and concrete." \
  --count 2 \
  --max-tokens 2000 \
  --temperature 0 \
  --retries 2

target/debug/tentgent dataset validate /private/tmp/tentgent-dataset-synth-openai

target/debug/tentgent dataset eval /private/tmp/tentgent-dataset-synth-openai \
  -p openai \
  -m gpt-4.1-mini \
  -o /private/tmp/tentgent-dataset-eval-openai \
  --split train \
  --max-records 2 \
  --criteria "Check that records are useful, concise customer-support billing examples." \
  --max-tokens 1200 \
  --temperature 0
```

Observed result:

- `dataset synth` verified the OpenAI key from Keychain and generated
  `/private/tmp/tentgent-dataset-synth-openai/train.jsonl`.
- `dataset validate` passed with `valid 2 record(s) across 1 split(s)`,
  `tuning_ready: yes`, and `errors: 0`.
- The generated records parsed as JSONL with ids `train-001` and `train-002`.
- `dataset eval` verified the OpenAI key from Keychain and wrote
  `/private/tmp/tentgent-dataset-eval-openai/prompt.md` and
  `/private/tmp/tentgent-dataset-eval-openai/report.json`.
- `report.json` is a provider-output envelope with `provider`, `model`,
  `split`, `finish_reason`, `prompt_path`, and `report_text`. The provider
  returned concise JSON inside `report_text` with summary, issues, and
  recommendations.
- The CLI evaluation summary table currently shows `-` for parsed fields such
  as `reviewed`, `overall_score`, `output_dir`, and `report_json` because the
  report envelope does not include those display fields.

### Block 6: Keychain Secret Path

Status: passed for OpenAI.

Goal:

- Verify cloud server launch and daemon cloud requests can resolve provider
  credentials from Keychain when env vars are absent.

Notes:

- This may need sandbox escalation and user approval for Keychain access.

Commands run:

```bash
printenv OPENAI_API_KEY
printenv TENTGENT_OPENAI_API_KEY
printenv TENTGENT_DAEMON_TOKEN

env -u OPENAI_API_KEY -u TENTGENT_OPENAI_API_KEY -u TENTGENT_DAEMON_TOKEN \
  target/debug/tentgent server run openai:gpt-4.1-mini \
  --capability chat \
  --host 127.0.0.1 \
  --port 8895 \
  --home /private/tmp/tentgent-keychain-cloud-server \
  --detach

curl -sS http://127.0.0.1:8895/healthz
curl -sS http://127.0.0.1:8895/v1/chat ...
target/debug/tentgent server stop a85e98afbfb6 \
  --home /private/tmp/tentgent-keychain-cloud-server

env -u OPENAI_API_KEY -u TENTGENT_OPENAI_API_KEY -u TENTGENT_DAEMON_TOKEN \
  target/debug/tentgent daemon start \
  --home /private/tmp/tentgent-keychain-daemon \
  --host 127.0.0.1 \
  --port 8795

curl -sS http://127.0.0.1:8795/v1/status
curl -sS http://127.0.0.1:8795/v1/chat/completions ...
target/debug/tentgent daemon stop --home /private/tmp/tentgent-keychain-daemon
```

Observed result:

- `OPENAI_API_KEY`, `TENTGENT_OPENAI_API_KEY`, and
  `TENTGENT_DAEMON_TOKEN` were unset in the shell before testing.
- Direct cloud server launch was run with those env vars explicitly unset and
  printed `verified OpenAI key verified from keychain for cloud runtime`.
- Direct cloud server `/healthz` returned the expected OpenAI cloud runtime
  metadata.
- Direct cloud server `/v1/chat` returned HTTP 200 with
  `keychain-server-ok`, confirming the launched child could use the
  Keychain-resolved secret.
- Daemon launch was also run with the provider and daemon env vars explicitly
  unset.
- Daemon `/v1/status` returned `token_enabled: false`, the expected runtime
  home, and no warnings.
- Daemon `/v1/chat/completions` returned HTTP 200 with
  `keychain-daemon-ok`, confirming the daemon route could resolve the OpenAI
  key from Keychain at request time.
- In the Codex sandbox, stop commands again needed escalation to send TERM.
  Both test processes were stopped afterward, and ports `8895` and `8795` had
  no listeners.

## Known Follow-Ups

- Direct cloud server `stream=true` currently returns the Tentgent SSE shape but
  may coalesce provider output into one delta instead of native token-by-token
  streaming.
- Daemon compatibility chat routes remain local-first; direct cloud server
  worker is the main provider-compatible text/image surface.
