# Tentgent 日本語入口

Tentgent はローカル AI workflow operator です。現在の product surface は
`tentgent` CLI とローカル daemon REST API です。terminal UI command は
ありません。

英語ドキュメントが source of truth です。この日本語 README は短い入口だけを
保持します。install、version、API、contract、readiness の詳細は英語
ドキュメントを参照してください。

## Quick Start

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
```

最小 local workflow:

```bash
tentgent auth hf set
tentgent model pull google/gemma-3-1b-it
tentgent chat <model-ref> --message "user:Hello"
tentgent daemon start --host 127.0.0.1 --port 8790
```

## Docs

- 英語 source of truth: [README.md](../../../README.md)
- 完整な user docs: [docs/user/README.md](../../../docs/user/README.md)
- 1.0 readiness checklist:
  [docs/user/1.0-readiness.md](../../../docs/user/1.0-readiness.md)
- Install と upgrade: [docs/user/install.md](../../../docs/user/install.md)
- Version notes: [docs/user/version.md](../../../docs/user/version.md)
- CLI command examples: [docs/user/commands.md](../../../docs/user/commands.md)
- Runtime と diagnostics: [docs/user/runtime.md](../../../docs/user/runtime.md)
- HTTP API reference: [docs/user/api.md](../../../docs/user/api.md)
- Provider compatibility:
  [docs/user/provider-compatibility.md](../../../docs/user/provider-compatibility.md)
- Model support catalog:
  [docs/user/model-support-catalog.md](../../../docs/user/model-support-catalog.md)
- API surface stability contract:
  [docs/contracts/api-surface-stability.md](../../../docs/contracts/api-surface-stability.md)
- Provider secret contract:
  [docs/contracts/auth-secrets.md](../../../docs/contracts/auth-secrets.md)
- Developer docs: [docs/development/README.md](../../../docs/development/README.md)

## Languages

- 繁體中文: [docs/i18n/zh-TW/README.md](../zh-TW/README.md)
- 日本語: [docs/i18n/ja/README.md](./README.md)
- Localized docs router: [docs/i18n/README.md](../README.md)
