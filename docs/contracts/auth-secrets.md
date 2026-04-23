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
- Commands that resolve provider secrets, such as `auth <provider>` status checks or `model pull` for Hugging Face, may trigger the prompt when no environment-variable override is present.
- Environment-variable overrides should bypass Keychain reads because secret resolution prefers `.env/env` first and the system keychain second.

## CLI Surface

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

- Show whether `.env/env` is present.
- Show whether a keychain entry is present.
- Show the effective source after applying resolution order.
- Show validation as `verified`, `invalid`, `unknown`, or `missing`.
- Do not print the secret value.
