# Development

This document collects repository-local developer commands that are useful while Tentgent is still source-first.

For user-facing setup and usage, start with the root [README.md](../../README.md).

Run any command with `--help` or `-h` to see available short option aliases for that command.

## Local Runtime Home

Run manual tests from the repository root.

Use one stable repository-local runtime home:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Keep only `.tentgent-test/` as the long-lived local test store. Temporary one-off `.tentgent-*` directories should be deleted after experiments.

Installed binaries should fall back to the default platform-managed runtime home when `TENTGENT_HOME` and other path overrides are not set.

## Build And Check

Build the Rust workspace:

```bash
cargo build --workspace
```

Check the Rust workspace:

```bash
cargo check --workspace
```

Run Rust tests:

```bash
cargo test --workspace
```

Some kernel tests intentionally read the current machine, such as platform
fact probing. Use `-- --show-output` when you need successful live-machine
tests to print what they observed:

```bash
cargo test -p tentgent-kernel -- --show-output
```

CI runners may not have GPUs, CUDA, Metal, Keychain entries, Linux
Secret Service/D-Bus session state, provider tokens, or a managed Python
runtime. Live-machine tests should print the observed facts and treat missing
optional local capabilities as data, not as a failure, unless the test
explicitly provisions that dependency.

The kernel keychain smoke test is opt-in because it may prompt the platform
credential UI. To run it locally and print the observed presence state without
printing any secret value:

```bash
TENTGENT_RUN_KEYCHAIN_TESTS=1 cargo test -p tentgent-kernel -- --show-output
```

Run Python unit tests that do not require provider network access:

```bash
uv run --project python/tentgent-model-runtime pytest
```

Use the Makefile wrappers:

```bash
make check
make run-cli ARGS='--help'
```

## Current CI/CD

The repository currently has one GitHub Actions workflow:
`.github/workflows/release.yml`.

It runs on `v*.*.*` tag pushes and manual `workflow_dispatch`. The package job
builds release artifacts on native runners for macOS Apple Silicon, macOS
Intel, Linux x86_64, and Windows x86_64, then uploads the archives and
checksums. The release job downloads those artifacts, prepares installer
assets and release notes, creates or updates the GitHub Release, and verifies
prerelease/latest release state.

macOS package jobs use the `apple-developer` GitHub Actions environment with
`deployment: false`. They import an Apple Developer ID Application certificate
from environment secrets, sign the `tentgent` binary with hardened runtime and
a timestamp, submit the package contents to Apple notarization, and verify the
signed executable with strict `codesign` verification before uploading
artifacts. The macOS release asset names stay `.tar.gz`; the workflow creates a
temporary zip only for Apple notarization submission. Bare CLI executables are
not app bundles, so the workflow does not use `spctl -t exec` as the release
gate.

The Linux x86_64 package job installs `libdbus-1-dev` and `pkg-config` before
packaging because the native Linux keychain backend links `libdbus-sys` through
the Secret Service/D-Bus stack.

The current release workflow does not run `cargo fmt`, `cargo check`,
`cargo test`, or Python unit tests before packaging. `scripts/package-local.sh`
performs `cargo build --release --bin tentgent` as part of artifact packaging.

Check release tag parsing and prerelease flag helpers:

```bash
bash scripts/test-release-metadata.sh
bash scripts/test-package-python-layout.sh
bash -n scripts/release-metadata.sh
bash -n scripts/test-release-metadata.sh
bash -n scripts/test-package-python-layout.sh
bash -n scripts/macos-import-codesign-certificate.sh
bash -n scripts/macos-notarize-package.sh
```

Required GitHub Actions secrets for the macOS signing path:

- `APPLE_TEAM_ID`
- `APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_BASE64`
- `APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_PASSWORD`
- `APPLE_NOTARY_KEY_ID`
- `APPLE_NOTARY_ISSUER_ID`
- `APPLE_NOTARY_KEY_BASE64`
- `APPLE_KEYCHAIN_PASSWORD`

`APPLE_CODESIGN_IDENTITY` is optional. When it is absent, the macOS import
script detects the first `Developer ID Application:` identity from the imported
temporary keychain.

Update the project Homebrew tap after a stable GitHub Release is published:

```bash
bash scripts/update-homebrew-formula.sh --tag v0.3.3
```

The helper reads release `checksums.txt`, updates the local
`hiroliang/tap` formula checkout, and prints the tap validation commands. It is
edit-only: run `brew audit`, `brew install`, `brew test`, and tap git
commit/push manually after reviewing the diff. Use `--dry-run` to preview the
formula patch without writing.

Check the tap updater itself:

```bash
bash scripts/test-update-homebrew-formula.sh
bash -n scripts/update-homebrew-formula.sh
bash -n scripts/test-update-homebrew-formula.sh
```

## CLI Help

Inspect layered help:

```bash
make run-cli ARGS='auth --help'
make run-cli ARGS='model --help'
make run-cli ARGS='adapter --help'
make run-cli ARGS='dataset eval --help'
make run-cli ARGS='server run --help'
```

## Daemon Development

Use a token-enabled daemon when checking auth-required behavior:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
cargo run -- daemon start --host 127.0.0.1 --port 8790
```

`GET /healthz` remains public. `/v1/status` returns `401` without a valid bearer
token when daemon auth is enabled.

Useful daemon lifecycle commands:

```bash
cargo run -- daemon start --host 127.0.0.1 --port 8790
cargo run -- daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
cargo run -- daemon stop
```

`daemon status` resolves daemon paths in read-only mode before any stale-metadata
cleanup, so checking a deleted or moved runtime home should not recreate it. If
status reports `runtime_home_missing`, `runtime_dir_missing`, or stale metadata
warnings, confirm the listener or pid manually before terminating any process.

Detached daemon logs are written under the resolved runtime home:

```text
logs/daemon.stdout.log
logs/daemon.stderr.log
```

Runtime footprint smoke:

```bash
cargo run -- runtime status
cargo run -- doctor
```

These commands report bounded, human-readable size estimates for the managed
Python environment and bootstrap caches. `runtime/bootstrap/uv-cache` is
safe-to-recreate package/cache data; remove it manually only when no installer
or Python bootstrap process is running. Do not remove `runtime/python-env`
unless intentionally repairing or reinstalling the managed Python runtime.

## Media Endpoint Smoke Tests

Use these checks when changing daemon upload handling, media routes, REST
errors, or user-facing media docs. They do not require a real model because
the intentionally tiny upload cap fails the request before model resolution.

Start a foreground daemon with an isolated runtime home and a tiny media upload
limit:

```bash
TENTGENT_HOME=/private/tmp/tentgent-media-limit-smoke \
TENTGENT_MEDIA_UPLOAD_MAX_BYTES=16 \
cargo run -p tentgent-cli -- daemon run --host 127.0.0.1 --port 8792
```

From another terminal, verify native vision chat upload errors:

```bash
curl -sS -i http://127.0.0.1:8792/v1/vision/chat \
  -F model_ref=missing \
  -F prompt='Describe this image.' \
  -F image=@test-data/test_image.png
```

Expected result:

```text
HTTP/1.1 413 Payload Too Large
{"error":"upload_too_large", ...}
```

Verify audio transcription upload errors:

```bash
curl -sS -i http://127.0.0.1:8792/v1/audio/transcriptions/job \
  -F model_ref=missing \
  -F output_format=text \
  -F file=@test-data/we_go_up.mp3
```

Expected result is also HTTP `413` with `upload_too_large`. Stop the foreground
daemon with Ctrl-C after the smoke checks.

For end-to-end model execution, keep using [docs/user/model-fixtures.md](../user/model-fixtures.md).

## Auth Commands

```bash
make run-cli ARGS='auth hf'
make run-cli ARGS='auth hf set'
make run-cli ARGS='auth openai'
make run-cli ARGS='auth anthropic'
make run-cli ARGS='auth gemini'
```

## Model Commands

Pull test models:

```bash
./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

Inspect model state:

```bash
./target/debug/tentgent model ls
./target/debug/tentgent model inspect <model-ref>
```

Remove a model:

```bash
./target/debug/tentgent model rm <model-ref>
```

Tentgent blocks model removal while any stored server spec still references that model.

## Adapter Commands

Import a local adapter:

```bash
./target/debug/tentgent adapter add test-data/exp_v2 --base-model-ref <model-ref>
```

Pull a PEFT adapter from Hugging Face:

```bash
./target/debug/tentgent adapter pull peft-internal-testing/tiny_T5ForSeq2SeqLM-lora
```

Bind an imported adapter:

```bash
./target/debug/tentgent adapter bind <adapter-ref> --base-model-ref <model-ref>
```

Inspect adapter state:

```bash
./target/debug/tentgent adapter ls
./target/debug/tentgent adapter inspect <adapter-ref>
```

Remove an adapter:

```bash
./target/debug/tentgent adapter rm <adapter-ref>
```

## Dataset Commands

Import a local `.jsonl` file or dataset directory:

```bash
./target/debug/tentgent dataset validate /path/to/dataset.jsonl
./target/debug/tentgent dataset validate /path/to/dataset-dir
./target/debug/tentgent dataset template -t chat -l zh-TW -o /path/to/dataset-template.md
./target/debug/tentgent dataset add /path/to/dataset.jsonl
./target/debug/tentgent dataset add /path/to/dataset-dir
```

Use `dataset template` to generate the manual prompt for OpenAI, Claude, Gemini, or
another agent. Its `--task` and `--language` options are prompt hints, not
schema changes. Provider-backed `dataset synth` and `dataset eval` use the Rust
cloud provider client. Use `dataset validate` before `dataset add` when working
with generated JSONL.

The HTTP daemon exposes deterministic dataset tooling for local integrations:

```bash
curl -sS http://127.0.0.1:8790/v1/datasets/validate \
  -H 'Content-Type: application/json' \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/datasets/template \
  -H 'Content-Type: application/json' \
  -d '{"task":"support","language":"zh-TW"}'
curl -sS http://127.0.0.1:8790/v1/datasets/synth \
  -H 'Content-Type: application/json' \
  -d '{"print_prompt":true,"brief":"Generate support examples in Traditional Chinese.","split":"train","count":20}'
curl -sS http://127.0.0.1:8790/v1/datasets/eval \
  -H 'Content-Type: application/json' \
  -d '{"input_content":"{\"schema\":\"tentgent.chat.v1\",\"messages\":[{\"role\":\"user\",\"content\":\"Hi\"},{\"role\":\"assistant\",\"content\":\"Hello\"}]}\n","provider":"openai","model":"gpt-4.1-mini","output_path":"/absolute/path/on/daemon-host/eval-report","max_records":1}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/export \
  -H 'Content-Type: application/json' \
  -d '{"output_path":"/absolute/path/on/daemon-host/work-dir"}'
curl -sS http://127.0.0.1:8790/v1/datasets/<dataset-ref>/diff \
  -H 'Content-Type: application/json' \
  -d '{"right_dataset_ref":"<other-dataset-ref>"}'
```

Validation failures are tool results: the daemon returns `200` with
`valid:false` when the request is well-formed but the dataset schema is invalid.
All paths are resolved on the daemon host filesystem. Cloud synth/eval HTTP
calls are synchronous provider workflows; prompt-only synth does not require
auth, while provider synth/eval may send selected path or content data to the
configured provider and return debug artifact paths on failure.

Inspect dataset state:

```bash
./target/debug/tentgent dataset ls
./target/debug/tentgent dataset inspect <dataset-ref>
./target/debug/tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
./target/debug/tentgent dataset diff <dataset-ref> -p /path/to/work-dir
./target/debug/tentgent dataset rm <dataset-ref>
```

Dataset directories are marked tuning-ready when a root `train.jsonl` exists. `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json` are detected when present.

Export a managed dataset to a working copy:

```bash
./target/debug/tentgent dataset export <dataset-ref> /path/to/work-dir
```

Edit the working copy, then import it again with `dataset add` to create a new dataset reference.

## Session Commands

Create and mutate local chat transcript sessions:

```bash
./target/debug/tentgent session create --title "Planning" --tag draft
./target/debug/tentgent session ls
./target/debug/tentgent session append <session-ref> --role user --content "Hello"
./target/debug/tentgent session append <session-ref> --role user --content "Hello" --compaction-server <server-ref>
./target/debug/tentgent session compact <session-ref> --server <server-ref>
./target/debug/tentgent session messages <session-ref> --tail 100
./target/debug/tentgent session update <session-ref> --title "Planning v2"
./target/debug/tentgent chat <model-ref> --session <session-ref> --message "user:Continue."
./target/debug/tentgent session rm <session-ref>
```

The HTTP daemon exposes the same session store mutations:

```bash
curl -sS http://127.0.0.1:8790/v1/sessions \
  -H 'Content-Type: application/json' \
  -d '{"title":"Planning","tags":["draft"]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/messages \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}]}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref>/compact \
  -H 'Content-Type: application/json' \
  -d '{"server_ref":"<server-ref>","keep_recent_messages":49}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> \
  -X PATCH \
  -H 'Content-Type: application/json' \
  -d '{"title":"Planning v2"}'
curl -sS http://127.0.0.1:8790/v1/sessions/<session-ref> -X DELETE
```

Session deletion is permanent. Chat remains stateless unless `--session` or
`session_ref` is provided. Session-aware chat holds the session lock until the
assistant reply is recorded, so same-session turns are serialized. Sessions are
bounded to 50 persisted messages; compaction may rewrite older transcript
messages into a generated summary message.

## Train Commands

Create a managed LoRA training plan without executing it:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
```

Force a backend during plan creation:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --backend mlx
```

Override selected defaults:

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --rank 8 \
  --learning-rate 0.00008 \
  --max-steps 320 \
  --review
```

Inspect stored plans:

```bash
./target/debug/tentgent train lora plan ls
./target/debug/tentgent train lora plan inspect <plan-ref>
./target/debug/tentgent train lora plan rm <plan-ref>
```

Daemon train-plan parity:

```bash
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/preview \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","dataset_ref":"<dataset-ref>","backend":"auto","overrides":{"rank":8,"max_steps":100}}'

curl -sS http://127.0.0.1:8790/v1/train/lora/plans \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","dataset_ref":"<dataset-ref>","backend":"auto"}'

curl -sS http://127.0.0.1:8790/v1/train/lora/plans
curl -sS http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>
curl -sS -X DELETE http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>
```

HTTP plan deletion refuses plans with run records. The daemon can also start
and observe runs:

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/train/lora/plans/<plan-ref>/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/runs
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/metrics
curl -sS http://127.0.0.1:8790/v1/train/lora/runs/<run-ref>/logs/raw
```

Run the current orchestration scaffold:

```bash
./target/debug/tentgent train lora run <plan-ref>
```

## Chat Commands

Run the Python model runtime directly:

```bash
python/tentgent-model-runtime/.venv/bin/tentgent-model-runtime-daemon \
  --model-ref <model-ref> \
  --home "$PWD/.tentgent-test" \
  --capability chat \
  --host 127.0.0.1 \
  --port 8780 \
  --lazy-load
```

Run Rust-wrapped chat:

```bash
./target/debug/tentgent chat <model-ref> \
  --home "$PWD/.tentgent-test" \
  --message "user:Hello there"
```

Stream generated text:

```bash
./target/debug/tentgent chat <model-ref> \
  --home "$PWD/.tentgent-test" \
  --message "system:You are terse." \
  --message "user:Hello there" \
  --stream
```

Message inputs accept `role:content` for ordered context. Supported roles are `system`, `user`, and `assistant`. If no role prefix is present, Tentgent treats the message as `user`.

## Server Commands

Run a foreground server:

```bash
./target/debug/tentgent server run <model-ref> \
  --home "$PWD/.tentgent-test" \
  --host 127.0.0.1 \
  --port 8780 \
  --lazy-load
```

Run a detached server:

```bash
./target/debug/tentgent server run <model-ref> \
  --home "$PWD/.tentgent-test" \
  --host 127.0.0.1 \
  --port 8780 \
  --lazy-load \
  --detach
```

Cloud provider servers launch Rust workers, for example
`tentgent server run gemini:gemini-2.0-flash --port 8785`.

Inspect and manage servers:

```bash
./target/debug/tentgent server ls --home "$PWD/.tentgent-test"
./target/debug/tentgent server ps --home "$PWD/.tentgent-test"
./target/debug/tentgent server inspect <server-ref> --home "$PWD/.tentgent-test"
./target/debug/tentgent server start <server-ref> --home "$PWD/.tentgent-test"
./target/debug/tentgent server stop <server-ref> --home "$PWD/.tentgent-test"
./target/debug/tentgent server rm <server-ref> --home "$PWD/.tentgent-test"
```

Add `--details` to `server start`, `server stop`, or `server rm` when you want a full inspection table.

## Python Server Direct Entry

Exercise the local Python model runtime daemon directly:

```bash
uv run --project python/tentgent-model-runtime tentgent-model-runtime-daemon \
  --server-ref <server-ref> \
  --model-ref <model-ref> \
  --home "$TENTGENT_HOME" \
  --capability chat \
  --host 127.0.0.1 \
  --port 8780 \
  --idle-keep-alive-seconds -1 \
  --lazy-load
```

Cloud provider servers do not use this Python entry point. They launch Rust
workers from the `tentgent` binary and call provider APIs through the Rust cloud
client.

The server exposes:

- `GET /healthz`
- `POST /v1/chat`
- `POST /v1/embeddings` when launched with `--capability embedding`
- `POST /v1/rerank` when launched with `--capability rerank`
- `POST /v1/audio/transcriptions` when launched with
  `--capability audio-transcription`
- `POST /v1/audio/speech` when launched with `--capability audio-speech`
- `POST /v1/vision/chat` when launched with `--capability vision-chat`
- `POST /v1/video/understanding` when launched with
  `--capability video-understanding`
- `POST /v1/images/generations`, `transforms`, `inpaint`, and `control` when
  launched with `--capability image-generation`

HTTP `stream=true` returns Server-Sent Events for local runtimes, compatible
local adapters, and OpenAI, Anthropic, or Gemini cloud provider runtimes.

Smoke-test streaming with:

```bash
curl -N http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":32,"stream":true}'
```

## Rust HTTP Daemon Entry

Start the daemon from the CLI:

```bash
cargo run -- daemon start --host 127.0.0.1 --port 8790
cargo run -- daemon status
```

Loopback daemon binds can run without auth for development. To exercise the
local bearer-token guard:

```bash
export TENTGENT_DAEMON_TOKEN='<local-token>'
cargo run -- daemon start --host 127.0.0.1 --port 8790
```

When the token is enabled, add
`-H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"` to every daemon `/v1/*`
request. `GET /healthz` stays public.

`daemon start` and `daemon run --detach` share one detached launch path. The
parent process waits up to five seconds for `GET /healthz`; if readiness times
out, the output includes the resolved daemon URL, runtime home, and daemon
stdout/stderr log paths. If `TENTGENT_DAEMON_TOKEN` is set, `/v1/status` is used
only as a stronger confirmation; a `401` does not override successful
`/healthz` readiness.

Check, call, or stop it:

```bash
cargo run -- daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/auth/openai \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/doctor \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN"
curl -sS http://127.0.0.1:8790/v1/daemon/logs
curl -sS 'http://127.0.0.1:8790/v1/daemon/logs/stderr?tail_bytes=4096'
curl -sS http://127.0.0.1:8790/v1/models
curl -sS http://127.0.0.1:8790/v1/adapters
curl -sS http://127.0.0.1:8790/v1/datasets
curl -sS http://127.0.0.1:8790/v1/servers
curl -sS http://127.0.0.1:8790/v1/servers \
  -X POST \
  -H 'Content-Type: application/json' \
  -d '{"runtime_ref":"openai:gpt-4.1-mini","host":"127.0.0.1","port":8780}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/start \
  -X POST \
  -H 'Content-Type: application/json' \
  -d '{"wait_ready":true,"timeout_seconds":30}'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/health
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/logs
curl -sS 'http://127.0.0.1:8790/v1/servers/<server-ref>/logs/stderr?tail_bytes=4096'
curl -sS http://127.0.0.1:8790/v1/models/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/model"}'
curl -sS http://127.0.0.1:8790/v1/adapters/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/adapter","base_model_ref":"<model-ref>"}'
curl -sS http://127.0.0.1:8790/v1/adapters/<adapter-ref>/bind \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"base_model_ref":"<model-ref>"}'
curl -sS http://127.0.0.1:8790/v1/datasets/import \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "server_ref": "<server-ref>",
    "session_ref": "<session-ref>",
    "max_session_messages": 50,
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0
  }'
curl -sS -N http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "server_ref": "<server-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'
curl -sS http://127.0.0.1:8790/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{
    "model": "<server-ref>",
    "session_ref": "<session-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0
  }'
curl -sS -N http://127.0.0.1:8790/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{
    "model": "<server-ref>",
    "messages": [
      {"role": "user", "content": "Say hello in Traditional Chinese."}
    ],
    "max_tokens": 64,
    "temperature": 0.0,
    "stream": true
  }'
curl -sS http://127.0.0.1:8790/v1/servers/<server-ref>/stop -X POST
curl -sS http://127.0.0.1:8790/v1/daemon/shutdown \
  -X POST \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -d '{}'
cargo run -- daemon stop
```

For foreground debugging, use:

```bash
cargo run -- daemon run --host 127.0.0.1 --port 8790
```

All daemon CLI launch paths reject wildcard or non-loopback binds without
`TENTGENT_DAEMON_TOKEN` unless `--allow-unsafe-bind` is passed explicitly.
Detached daemon children inherit daemon configuration environment variables,
including `TENTGENT_DAEMON_TOKEN`; local model-server proxy children remove
that token before launch.

At this stage the daemon records process metadata and serves `GET /healthz`,
`GET /v1/status`, and read-only discovery endpoints for models, adapters,
datasets, server specs, controlled server lifecycle mutations, and
`POST /v1/chat` proxying to already-running model-bound server ports.
`POST /v1/chat/completions` adds a limited OpenAI-style success wrapper for
basic chat-completion clients; its `model` field selects a Tentgent server ref
or unique prefix, not a provider model name. Both chat routes can optionally use
`session_ref` for bounded context and transcript recording. Persisted session
transcripts are capped at 50 messages and may compact older messages into one
summary message. Use
`GET /v1/servers/<server-ref>/health` to distinguish process state from target
HTTP reachability before sending chat. Use the daemon and server log diagnostics
endpoints to inspect fixed stdout/stderr log paths without accepting arbitrary
filesystem paths. Store import, pull, inspect, and remove parity is available through
`POST /v1/models/import`, `POST /v1/models/pull`,
`POST /v1/adapters/import`, `POST /v1/adapters/pull`,
`POST /v1/adapters/<ref>/bind`, `POST /v1/datasets/import`,
`GET`/`DELETE /v1/models/<ref>`, `/v1/adapters/<ref>`, `/v1/datasets/<ref>`,
and `DELETE /v1/servers/<ref>`; server delete removes stopped specs only.
Import paths are absolute paths on the daemon host. Pull endpoints are
synchronous compatibility calls. Background variants use
`POST /v1/models/pull/jobs`, `POST /v1/models/import/jobs`, `POST
/v1/adapters/pull/jobs`, `POST /v1/adapters/import/jobs`, `POST
/v1/datasets/import/jobs`, `POST /v1/datasets/synth/jobs`, and `POST
/v1/datasets/eval/jobs`, then expose progress through `GET /v1/jobs` and `GET
/v1/jobs/<job-id>`. No cancel route exists in Slice 4.1.
Request logs are emitted to stderr with peer, method, path, status, and elapsed
time fields. Auth failures never log token or header values.
Auth status is read-only and reports local env/keychain presence without
provider network validation. HTTP doctor is observational only and does not run
`doctor --fix` behavior. Daemon shutdown requires an enabled bearer token even
on loopback and stops only the daemon process.

The active Rust daemon host lives in `src/tentgent-daemon/`:

- `src/bootstrap/` builds daemon config, logging, runtime layout, and services.
- `src/app/` owns shared daemon process state.
- `src/transport/rest/` owns Axum routing, bearer-token middleware, and REST
  transport startup.
- `src/handlers/rest/` owns endpoint handlers split by feature.
- `src/runtime/` owns daemon-local job and scheduler state.

Model-bound server launch helpers are kernel-owned and exposed through the CLI
and daemon adapters. Local starts launch the Rust proxy, which uses the shared
Python model runtime daemon supervisor on demand. The removed `tentgent-http`
and `tentgent-core` crates are no longer workspace members.

## Documentation Rules

- Keep the root README user-facing.
- Put developer command detail in this file.
- Put interface contracts under `docs/contracts/`.
- Put unfinished staged plans under `docs/plans/`.
- Move completed plans to `docs/plans/archive/`.
- Keep Markdown concise and split by concern.
