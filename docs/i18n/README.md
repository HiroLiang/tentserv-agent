# Localized Docs

This directory holds localized Markdown that mirrors English source documents from the repository root or other English-first documentation subtrees.

## Available Languages

| Language | Entry Point | Status |
| --- | --- | --- |
| English | [../../README.md](../../README.md) | Source of truth |
| Traditional Chinese | [zh-TW/README.md](./zh-TW/README.md) | Localized quick start |
| Japanese | [ja/README.md](./ja/README.md) | Localized quick start |

Detailed user docs currently live in English under
[docs/user/](../user/README.md). Localized README files link back to those
source documents when no localized counterpart exists yet.

## Quick Navigation

- Start using Tentgent: [../../README.md](../../README.md#quick-start)
- Full command examples: [../user/commands.md](../user/commands.md)
- Install and upgrade: [../user/install.md](../user/install.md)
- Runtime home and diagnostics: [../user/runtime.md](../user/runtime.md)
- Current version notes: [../user/version.md](../user/version.md)

## Rules

- English documents outside `docs/i18n/` remain the source of truth.
- Keep localized files aligned with their English counterparts.
- Add localized files here instead of mixing multiple languages into root-level Markdown.
