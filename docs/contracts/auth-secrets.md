# Auth Secrets

This document defines how Tentgent should resolve, store, and validate provider API keys.

## Providers

- Hugging Face
- OpenAI
- Anthropic

## Resolution Order

Resolve secrets in this order:

1. `.env/env`
2. system keychain
3. none

Use these environment variables:

- `HF_TOKEN`
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`

`.env` loading is allowed for development convenience and should override process environment variables for the current Tentgent process when present.
The default `.env` lookup is the current process working directory and its
parents. It is not `TENTGENT_HOME/.env`. Do not make `TENTGENT_HOME` an
implicit plaintext secret directory; support for an explicit auth env file must
be opt-in and represented by auth env-probe policy.

## Persistence Rule

- Never write provider secrets to the repository.
- Never write provider secrets to `config.toml`.
- Persist provider secrets in the system keychain.
- Use non-secret config files only for non-secret auth preferences such as provider enablement or endpoint selection.

## Keychain Prompt Rule

- On macOS, a system Keychain prompt is expected when Tentgent reads a stored secret from the system keychain.
- Commands that only inspect local model-store metadata, such as `model ls` and `model inspect`, should not trigger provider-secret reads.
- Commands that resolve provider secrets, such as `auth <provider>` status checks, `model pull` for Hugging Face, or cloud provider server launch preflight, may trigger the prompt when no environment-variable override is present.
- Environment-variable overrides should bypass Keychain reads because secret resolution prefers `.env/env` first and the system keychain second.
- Auth env-secret lookup belongs behind an auth env probe. The probe may read
  process environment only, search the current working directory for `.env`, or
  use an explicit env file depending on policy.
- HTTP/TUI/CLI status flows that do not explicitly validate or launch provider
  work should prefer non-prompting status: report environment presence and
  recorded/cached keychain presence when available, but do not read the
  Keychain secret by default.
- Secret-use flows such as provider validation, Hugging Face pulls, cloud
  server launch, and dataset cloud generation may read the Keychain secret.
  These flows should prefer biometric unlock when the platform backend can
  support it and may use a short process-session cache so one accepted unlock
  can serve repeated operations in the same CLI/daemon/TUI process.
- Process-session secret cache is memory-only, TTL-bounded, and must never be
  persisted under `TENTGENT_HOME` or config. Secret wrappers should clear their
  owned memory on drop where the Rust type can reasonably guarantee it.
- Biometric unlock is a preference, not a cross-platform guarantee. The current
  generic keyring path may fall back to the operating system's default Keychain
  prompt behavior.
- Prompt planning belongs in auth infra. The generic planner must report when
  biometric unlock was requested but the current backend cannot honor it, so
  CLI/TUI can explain why the operating-system default prompt is used.

## Kernel Use Cases

Auth workflows are capability-sized modules under `features/auth/usecases/`:

- status: assemble provider status without reading Keychain secret material by
  default.
- resolution: resolve the effective secret as `.env/env`, process-session
  TTL cache, then Keychain.
- mutation: set/remove local Keychain secrets and keep non-secret metadata and
  process cache consistent.
- validation: resolve a secret, call a provider validator, and record the
  non-secret validation state.

Use-case ports live in `features/auth/usecases/port.rs`. Lower-level ports for
env probing, Keychain storage, validation HTTP, metadata, cache, and prompt
planning stay in `features/auth/ports.rs`.

## Cloud Server Launch Rule

- Cloud provider server specs must not store provider secrets.
- `server run` and `server start` must resolve and validate the effective provider secret before starting cloud provider runtime work.
- Missing, invalid, and unknown validation states must fail before runtime launch.
- Cloud runtime launch passes the selected secret to the child process only through the provider's standard environment variable.

## CLI Surface

- `tentgent auth status`
- `tentgent auth hf`
- `tentgent auth hf set`
- `tentgent auth hf rm`
- `tentgent auth openai`
- `tentgent auth openai set`
- `tentgent auth openai rm`
- `tentgent auth anthropic`
- `tentgent auth anthropic set`
- `tentgent auth anthropic rm`

## TUI Surface

`tentgent tui` may expose guarded local provider setup through the same
`AuthManager` and system Keychain path used by the CLI.

Rules:

- Provider key set/remove is local-only and must not add daemon HTTP mutation
  routes.
- Provider secrets must be masked during input.
- Provider secrets must never be displayed, logged, serialized, or written to
  `config.toml`.
- Environment-variable credentials remain the effective source when present;
  the TUI must show that env overrides keychain.
- Removal must be confirmable and name the provider/keychain entry affected.
- Slice 1 does not perform provider network validation by default after set or
  remove; it shows local status.

## Validation Endpoints

- Hugging Face: `GET https://huggingface.co/api/whoami-v2`
- OpenAI: `GET https://api.openai.com/v1/models`
- Anthropic: `GET https://api.anthropic.com/v1/models` with `anthropic-version: 2023-06-01`

## Output Rule

- `auth status` should show every provider in one table.
- Show whether `.env/env` is present.
- Show whether a keychain entry is present.
- Show the effective source after applying resolution order.
- Show validation as `verified`, `invalid`, `unknown`, or `missing`.
- Do not print the secret value.

## HTTP Daemon Auth Status

The daemon exposes read-only auth status through:

```text
GET /v1/auth
GET /v1/auth/{provider}
```

This HTTP surface is diagnostic-only:

- It never returns provider secret values.
- It does not set or remove provider secrets.
- It does not call provider validation endpoints by default.
- It reports validation as `not_checked`.
- Environment-variable credentials bypass Keychain reads.
- If no env override exists, Keychain presence checks may trigger the platform
  Keychain prompt.

Provider key mutation remains local-only through CLI or guarded TUI
`AuthManager`/Keychain flows. Daemon HTTP secret mutation remains out of scope
until a stricter HTTP secret mutation model is designed.
