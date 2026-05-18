# Auth Secrets

This document defines how Tentgent should resolve, store, and validate provider API keys.

## Providers

- Hugging Face
- OpenAI
- Anthropic
- Gemini

## Resolution Order

Resolve secrets in this order:

1. `.env/env`
2. system keychain/secret store
3. none

Secret-use flows may also accept an explicit one-operation secret from their
input surface, such as a CLI prompt or a single HTTP request. That value is an
ephemeral override for that operation, not persistent local auth state.

Use these environment variables:

- `HF_TOKEN`
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `GEMINI_API_KEY`

`.env` loading is allowed for development convenience and should override process environment variables for the current Tentgent process when present.
The default `.env` lookup is the current process working directory and its
parents. It is not `TENTGENT_HOME/.env`. Do not make `TENTGENT_HOME` an
implicit plaintext secret directory; support for an explicit auth env file must
be opt-in and represented by auth env-probe policy.

## Persistence Rule

- Never write provider secrets to the repository.
- Never write provider secrets to `config.toml`.
- Persist provider secrets in the platform system secret store.
- Use non-secret config files only for non-secret auth preferences such as provider enablement or endpoint selection.

## System Secret Store Rule

Tentgent code uses `Keychain` as the cross-platform domain name for the native
secret store boundary.

- macOS uses the system Keychain.
- Windows uses Credential Manager through the native keyring backend.
- Linux uses the native keyring backend backed by persistent Secret
  Service/keyutils storage when available.
- Headless Linux and CI environments may not have D-Bus, Secret Service, or an
  unlocked user keyring. Missing local credential infrastructure should surface
  as unavailable/unknown state unless the test or command explicitly requires
  it.

## Unsupported Native Store Fallback Rule

If the current platform has no supported native secret store, or the store is
unavailable at runtime, Tentgent must not silently fall back to plaintext
persistence.

- `config.toml`, `TENTGENT_HOME`, and repository files must not receive provider
  secrets.
- CLI persistent set/remove commands should report that native secret storage is
  unsupported or unavailable. Secret-use commands may accept a masked or pasted
  one-operation secret and keep it only in process memory.
- HTTP secret-use endpoints may accept a per-request provider secret in a header
  or request body field. They must not accept provider secrets in query strings,
  persist them, return them, or promote them into daemon-global mutable state.
- Per-request and prompt-provided secrets may be represented as request-provided
  auth material in kernel ports/use cases. They may use the short process-session
  cache only when the caller explicitly opts into that behavior.
- Repeatable headless and CI flows should prefer environment variables or an
  explicit auth env file policy.

## Keychain Prompt Rule

- On macOS, Tentgent writes provider secrets with Security Framework
  user-presence access control. It prefers the Data Protection Keychain when
  the current process has the required entitlement, and otherwise uses the
  login Keychain with the same user-presence access control. If macOS rejects
  user-presence access control because the process lacks required signing
  entitlements, the store may fall back to a standard login Keychain entry so
  local auth remains usable. Reads of user-presence entries should let the
  system prefer Touch ID or another available user-presence mechanism and fall
  back to the system password prompt.
- On Windows and Linux, the operating system or desktop credential backend may
  use its own unlock prompt, session keyring, or credential UI.
- Commands that only inspect local model-store metadata, such as `model ls` and `model inspect`, should not trigger provider-secret reads.
- Commands that resolve provider secrets, such as `auth <provider>` status checks, `model pull` for Hugging Face, or cloud provider server launch preflight, may trigger the prompt when no environment-variable override is present.
- Environment-variable overrides should bypass Keychain reads because secret resolution prefers `.env/env` first and the system keychain second.
- Auth env-secret lookup belongs behind an auth env probe. The probe may read
  process environment only, search the current working directory for `.env`, or
  use an explicit env file depending on policy.
- CLI and daemon REST status flows that do not explicitly validate or launch provider
  work should prefer non-prompting status: report environment presence and
  recorded/cached keychain presence when available, but do not read the
  Keychain secret by default.
- Secret-use flows such as provider validation, Hugging Face pulls, cloud
  server launch, and dataset cloud generation may read the Keychain secret.
  They may use a short process-session cache so one accepted unlock can serve
  repeated operations in the same CLI or daemon process.
- Process-session secret cache is memory-only, TTL-bounded, and must never be
  persisted under `TENTGENT_HOME` or config. Secret wrappers should clear their
  owned memory on drop where the Rust type can reasonably guarantee it.
- Keychain unlock strategy belongs inside the Keychain secret store, not in
  user config or use-case request data. When a platform backend can request a
  non-password system unlock path, such as user presence or biometrics, the
  store should try that single path first and then fall back to the system
  password prompt if it is unavailable or rejected.
- Existing macOS entries written before this policy are treated as legacy
  entries. They may still read through the old prompt path until the provider
  secret is set again and rewritten with user-presence access control.
- Unsigned development binaries may be unable to create user-presence Keychain
  entries. That is a signing/entitlement constraint rather than a user-facing
  setting.
- The store must not iterate every possible biometric or unlock device, and
  CLI and daemon REST callers must not expose dynamic prompt preferences. Callers
  describe intent only: status, secret use, or validation.
- Rust `std` does not provide a biometric API. macOS uses
  `security-framework` directly for user-presence access control; Windows and
  Linux use native `keyring` backends and their operating-system prompt policy.

## Kernel Use Cases

Auth workflows are capability-sized modules under `features/auth/usecases/`:

- status: assemble provider status without reading Keychain secret material by
  default.
- resolution: resolve the effective secret as `.env/env`, process-session
  TTL cache, then Keychain. When a caller supplies a prompt-provided or
  request-provided secret, that explicit one-operation secret is resolved before
  env/cache/Keychain and carries its own non-persistent source.
- mutation: set/remove local Keychain secrets and keep non-secret metadata and
  process cache consistent.
- validation: resolve a secret, call a provider validator, and record the
  non-secret validation state.

Use-case ports live in `features/auth/usecases/port.rs`. Lower-level ports for
env probing, Keychain storage, validation HTTP, metadata, and cache stay in
`features/auth/ports.rs`.

Auth metadata persistence uses `TENTGENT_HOME/runtime/auth.toml`. That file is
auth-specific local state and must contain only non-secret metadata. It is
separate from user `config.toml` so auth state can evolve without turning
general config into a secret-adjacent persistence surface.

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
- `tentgent auth gemini`
- `tentgent auth gemini set`
- `tentgent auth gemini rm`

The CLI auth surface composes kernel auth use cases directly. It uses
`StdAuthStatusUseCase`, `StdAuthSecretMutationUseCase`, and
`StdAuthSecretValidationUseCase` with the shared system secret store and
`runtime/auth.toml` metadata store. CLI rendering must not manually persist
provider auth metadata or secret values outside those use-case boundaries.

## Validation Endpoints

- Hugging Face: `GET https://huggingface.co/api/whoami-v2`
- OpenAI: `GET https://api.openai.com/v1/models`
- Anthropic: `GET https://api.anthropic.com/v1/models` with `anthropic-version: 2023-06-01`
- Gemini: `GET https://generativelanguage.googleapis.com/v1beta/models?key=...`

Kernel validation infra uses `reqwest` behind `AuthSecretValidator`. Unit tests
should cover request URL/header construction and HTTP status mapping without
calling external provider endpoints by default. Live provider validation tests
must be opt-in and require explicit provider credentials.

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
- It does not accept per-request provider secrets.
- It does not call provider validation endpoints by default.
- It reports validation as `not_checked`.
- Environment-variable credentials bypass Keychain reads.
- If no env override exists, Keychain presence checks may trigger the platform
  Keychain prompt.

Provider key mutation remains local-only through CLI Keychain flows. Daemon HTTP
secret mutation remains out of scope until a stricter HTTP secret mutation model
is designed.
