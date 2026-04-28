# Tentgent

Tentgent 是以 Rust 為主的本地 operator CLI，搭配 Python daemon 層管理模型 runtime、adapter、LoRA training，以及長時間運行的本地 server。

目前 MVP 可以管理 provider key、下載並去重本地模型、匯入或下載 LoRA adapter、管理 dataset、執行單次聊天、訓練 LoRA adapter，以及提供本地 HTTP chat。

## 語言

- 英文 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](./README.md)
- 日文: [docs/i18n/ja/README.md](../ja/README.md)

## 安裝

建議 macOS 使用者從最新 GitHub Release 安裝：

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | sh
```

建議 Windows PowerShell 使用者從最新 GitHub Release 安裝：

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

若你想要可重現的固定版本安裝，請指定版本：

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.2/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.1.2/install.ps1 | iex
```

接著確認預設安裝位置在 `PATH` 中，並檢查 runtime：

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
tentgent doctor
```

升級方式是重新執行 installer。`TENTGENT_HOME` 下的使用者 runtime data 會被保留。

安裝、升級、固定版本與本機 package smoke test 請看 [docs/user/install.md](../../../docs/user/install.md)。

## 目前版本

`v0.1.2` 在第一個可安裝 MVP 上加入 cloud provider server routing 與 provider-assisted dataset workflows。

已包含：

- Hugging Face、OpenAI、Anthropic 的 provider auth key 管理
- content-addressed model、adapter、dataset stores
- OpenAI 與 Anthropic local server proxy runtimes
- dataset validation、prompt templates、provider synthesis、provider evaluation
- MLX、PEFT safetensors、llama-cpp GGUF 路徑的單次本地 chat
- 本地 HTTP chat server，包含 registry 與 process lifecycle commands
- managed LoRA train plans，以及可執行的 MLX / PEFT training loops
- 一般安裝用的 installer-managed Python runtime bootstrap

目前限制：

- macOS 與 Windows x86_64 是第一批 packaged install targets
- MLX 需要 Apple Silicon macOS
- HTTP chat streaming 已規劃但尚未實作
- macOS signing 與 notarization 會留到後續 slice

版本功能與限制請看 [docs/user/version.md](../../../docs/user/version.md)。

## 快速開始

下載小型模型：

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

執行單次聊天：

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

啟動本地 server：

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

常用指令、dataset flow、adapter flow、LoRA training 與 server smoke test 請看 [docs/user/commands.md](../../../docs/user/commands.md)。

## 開發

從 source build：

```bash
cargo build --workspace
./target/debug/tentgent doctor
```

測試時使用 repo-local runtime home：

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

開發者指令與 repository-local tests 請看 [docs/development/README.md](../../../docs/development/README.md)。

## 專案文件

- [docs/user/](../../../docs/user/README.md)
  使用者安裝、升級、版本、指令、runtime 與 Keychain 文件。
- [AGENTS.md](../../../AGENTS.md)
  Shared repository context 與 documentation routing。
- [CLAUDE.md](../../../CLAUDE.md)
  Agent workflows 與 role boundaries。
- [docs/contracts/](../../../docs/contracts/README.md)
  Cross-language interfaces 與 stable runtime contracts。
- [docs/plans/](../../../docs/plans/README.md)
  Active staged plans。

## License

此專案為 proprietary，保留所有權利。請見 [LICENSE](../../../LICENSE)。
