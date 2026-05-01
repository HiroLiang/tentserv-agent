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

Provider key mutation remains CLI-only until a stricter HTTP secret mutation
model is designed.
