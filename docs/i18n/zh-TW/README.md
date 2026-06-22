# Tentgent 中文入口

Tentgent 是本地 AI workflow operator。當前產品介面是 `tentgent` CLI 加本地
daemon REST API；沒有 terminal UI 指令。

英文文件是 source of truth。這份中文 README 只保留快速入口，詳細安裝、版本、
API、contract 與 readiness 內容請以英文文件為準。

## 快速開始

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
```

最小本地流程：

```bash
tentgent auth hf set
tentgent model pull google/gemma-3-1b-it
tentgent chat <model-ref> --message "user:Hello"
tentgent daemon start --host 127.0.0.1 --port 8790
```

## 文件入口

- 英文 source of truth: [README.md](../../../README.md)
- 完整使用者文件: [docs/user/README.md](../../../docs/user/README.md)
- 1.0 readiness checklist:
  [docs/user/1.0-readiness.md](../../../docs/user/1.0-readiness.md)
- 安裝與升級: [docs/user/install.md](../../../docs/user/install.md)
- 版本說明: [docs/user/version.md](../../../docs/user/version.md)
- CLI 指令範例: [docs/user/commands.md](../../../docs/user/commands.md)
- Runtime 與 diagnostics: [docs/user/runtime.md](../../../docs/user/runtime.md)
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

## 語言

- 繁體中文: [docs/i18n/zh-TW/README.md](./README.md)
- 日本語: [docs/i18n/ja/README.md](../ja/README.md)
- Localized docs router: [docs/i18n/README.md](../README.md)
