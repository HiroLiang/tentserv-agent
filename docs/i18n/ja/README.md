# Tentgent

Tentgent は Rust を主体としたローカル operator CLI で、Python daemon レイヤーを使って model runtime、adapter、LoRA training、長時間動作するローカル server を管理します。

現在の MVP は provider key の管理、ローカル model の取得と重複排除、LoRA adapter の import / pull、dataset 管理、単発 chat、LoRA adapter training、ローカル HTTP chat に対応しています。

## 言語

- 英語 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](../zh-TW/README.md)
- 日本語: [docs/i18n/ja/README.md](./README.md)

## インストール

macOS ユーザー向けの推奨インストールは、最新 GitHub Release から行います:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
```

Windows PowerShell ユーザー向けの推奨インストール:

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

再現可能なセットアップにしたい場合は、version を固定して install します:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.1/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.1/install.ps1 | iex
```

その後、デフォルトのインストール先を `PATH` に追加し、runtime を確認します:

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
tentgent doctor
```

upgrade は installer を再実行します。`TENTGENT_HOME` 配下の user runtime data は保持されます。

install、upgrade、pinned version、local package smoke test は [docs/user/install.md](../../../docs/user/install.md) を参照してください。

## 現在のバージョン

`v0.1.1` は macOS と Windows release artifacts を含む最初の installable MVP target です。

含まれる機能:

- Hugging Face、OpenAI、Anthropic の provider auth key 管理
- content-addressed model、adapter、dataset stores
- MLX、PEFT safetensors、llama-cpp GGUF path の one-shot local chat
- registry と process lifecycle commands を備えた local HTTP chat server
- managed LoRA train plans と実行可能な MLX / PEFT training loops
- 通常 install 用の installer-managed Python runtime bootstrap

現在の制限:

- macOS と Windows x86_64 が最初の packaged install targets
- MLX は Apple Silicon macOS が必要
- HTTP chat は現在 non-streaming
- macOS signing と notarization は後続 slice に延期

version feature list と known limits は [docs/user/version.md](../../../docs/user/version.md) を参照してください。

## Quick Start

小さな model を取得:

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

単発 chat を実行:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

ローカル server を起動:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

common commands、dataset flow、adapter flow、LoRA training、server smoke test は [docs/user/commands.md](../../../docs/user/commands.md) を参照してください。

## Development

source から build:

```bash
cargo build --workspace
./target/debug/tentgent doctor
```

テスト中は repository-local runtime home を使います:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

developer commands と repository-local tests は [docs/development/README.md](../../../docs/development/README.md) を参照してください。

## Project Docs

- [docs/user/](../../../docs/user/README.md)
  User install、upgrade、version、command、runtime、Keychain docs。
- [AGENTS.md](../../../AGENTS.md)
  Shared repository context と documentation routing。
- [CLAUDE.md](../../../CLAUDE.md)
  Agent workflows と role boundaries。
- [docs/contracts/](../../../docs/contracts/README.md)
  Cross-language interfaces と stable runtime contracts。
- [docs/plans/](../../../docs/plans/README.md)
  Active staged plans。

## License

This project is proprietary and all rights are reserved. See [LICENSE](../../../LICENSE).
