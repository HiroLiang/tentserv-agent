# Tentgent

Tentgent 是以 Rust 為主的本地 operator CLI，搭配 Python daemon 層管理模型 runtime、adapter、LoRA training、長時間運行的本地 server，以及本地 HTTP control plane。

目前 MVP 可以管理 provider key、下載並去重本地模型、匯入或下載 LoRA adapter、管理 dataset、執行單次聊天、訓練 LoRA adapter、提供本地 HTTP chat，並透過 daemon API expose 主要本地工作流程。

## 語言

- 英文 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](./README.md)
- 日文: [docs/i18n/ja/README.md](../ja/README.md)

## 安裝工具

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
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.1/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.1/install.ps1 | iex
```

接著確認預設安裝位置在 `PATH` 中，並檢查 runtime：

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
tentgent doctor
tentgent --version
```

升級方式是重新執行 installer。`TENTGENT_HOME` 下的使用者 runtime data 會被保留。

安裝、升級、固定版本、本機 package smoke test 與移除說明請看 [docs/user/install.md](../../../docs/user/install.md)。

## 設定 Key

檢查本機 runtime 與 provider key 狀態：

```bash
tentgent doctor
tentgent status
tentgent auth status
```

設定 provider key 到系統 Keychain：

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
```

也可以用環境變數或目前 process 讀取的 `.env`：

```bash
cat > .env <<'EOF'
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
EOF
```

provider secret resolution 與 Keychain boundaries 請看 [docs/contracts/auth-secrets.md](../../../docs/contracts/auth-secrets.md)。

## 匯入、拉取、移除模型

管理模型：

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent model ls
tentgent model inspect <model-ref>
tentgent model add /absolute/path/to/model
tentgent model rm <model-ref>
```

完整模型、adapter、dataset 與 chat 範例請看 [docs/user/commands.md](../../../docs/user/commands.md#models-and-chat)。

## 單次 Chat

不啟動 server，直接跑一次本地請求：

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

單次 chat message 格式與 adapter 範例請看 [docs/user/commands.md](../../../docs/user/commands.md#models-and-chat)。

## 起停 Server 並對 Server 聊天

啟動 model-bound local server：

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
curl -sS http://127.0.0.1:8780/healthz
```

啟動 cloud provider server：

```bash
tentgent server run openai:gpt-4.1-mini --host 127.0.0.1 --port 8780
tentgent server run claude:claude-sonnet-4-20250514 --host 127.0.0.1 --port 8781
```

直接對 server 聊天：

```bash
curl -sS http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

管理 detached servers：

```bash
tentgent server ls
tentgent server ps
tentgent server stop <server-ref>
```

Direct model-server chat 是 stateless。若要 session-aware chat，請走 daemon。server chat request 與 adapter rules 請看 [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md)。

## 起停 Daemon

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

需要 session-aware routing 時，透過 daemon 指到 selected server：

```bash
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"server_ref":"<server-ref>","messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

停止 daemon：

```bash
tentgent daemon stop
```

完整 daemon API、endpoint、response shape、auth 與 error mapping 請看 [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md)。

## 進入 TUI

```bash
tentgent tui
```

TUI 是 operator console，可用於 daemon discovery、chat、jobs、resources、store、server、training 與 guarded setup flows。

## 移除工具

只移除已安裝的工具本體，不刪使用者 runtime data：

```bash
rm -f "$HOME/.local/bin/tentgent"
rm -rf "$HOME/.local/share/tentgent"
```

可選擇清除 safe-to-recreate bootstrap cache：

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

除非你確定要刪掉 models、adapters、datasets、sessions、servers、train records 與其他本地 runtime data，否則不要刪 `TENTGENT_HOME`。移除與 runtime-home 細節請看 [docs/user/install.md](../../../docs/user/install.md) 與 [docs/user/runtime.md](../../../docs/user/runtime.md)。

## 版本說明

`v0.3.1` 是 0.3.x stable hotfix，補上 macOS release binary ad-hoc signing 與 installer 安裝後的 quarantine cleanup，降低下載安裝後被 macOS 直接 kill 的情況。

版本功能與限制請看 [docs/user/version.md](../../../docs/user/version.md)。

## 完整 CLI 指令文件

README 只保留最短入口。完整 CLI 指令文件請看 [docs/user/commands.md](../../../docs/user/commands.md)，涵蓋 TUI、auth、models、adapters、datasets、chat、servers、daemon、sessions 與 LoRA training。

## API 與 Contracts

詳細 contract 分散在 [docs/contracts/](../../../docs/contracts/README.md)，讓 README 保持容易掃描。

- [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md)
  完整本地 daemon API contract、endpoint、auth、response shape 與 error mapping。
- [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md)
  model-bound server chat request shape 與 adapter validation rules。
- [docs/contracts/session-store.md](../../../docs/contracts/session-store.md)
  session metadata、message records、mutation rules 與 bounded compaction。
- [docs/contracts/runtime-home.md](../../../docs/contracts/runtime-home.md)
  runtime home、store path、Python runtime 與環境變數 override rules。
- [docs/contracts/auth-secrets.md](../../../docs/contracts/auth-secrets.md)
  provider secret resolution、`.env` / env 行為與 Keychain boundaries。
- [docs/contracts/training-lora.md](../../../docs/contracts/training-lora.md)
  managed LoRA plan 與 run boundaries。

## 路徑與 `.env`

用 `TENTGENT_HOME` 移動一般 runtime state：

```bash
export TENTGENT_HOME="$HOME/.tentgent"
```

只移動特定 store 或 Python runtime：

```bash
export TENTGENT_MODELS_DIR="/Volumes/models/tentgent"
export TENTGENT_DATASETS_DIR="$HOME/datasets/tentgent"
export TENTGENT_PYTHON_DIR="$PWD/python/tentgent-daemon"
export TENTGENT_PYTHON_ENV_DIR="$PWD/python/tentgent-daemon/.venv"
```

Tentgent 會先讀 `.env` / env，再 fallback 到系統 Keychain。若要讓 `.env`
行為可預期，請從含有 `.env` 的目錄執行 `tentgent`，或直接在 shell export
變數。

更多 runtime home、Python runtime 與 Keychain prompt 說明請看 [docs/user/runtime.md](../../../docs/user/runtime.md)。

## 目前能力

- Hugging Face、OpenAI、Anthropic 的 provider auth key 管理
- content-addressed model、adapter、dataset stores
- OpenAI 與 Anthropic local server proxy runtimes
- dataset validation、prompt templates、multi-split provider synthesis、provider evaluation
- MLX、PEFT safetensors、llama-cpp GGUF 路徑的單次本地 chat
- 本地 HTTP daemon API，涵蓋 store、dataset、server、chat、training、diagnostics 與 bounded session workflows
- terminal UI operator console，可做 daemon discovery、chat、jobs、resources、store/server/training actions、session cleanup，以及受保護的本機 setup
- managed LoRA train plans、durable run records、metrics/log inspection，以及可執行的 MLX / PEFT training loops
- bounded transcript compaction 的本地 session，作為短期 working context
- 一般安裝用的 installer-managed Python runtime bootstrap

目前限制：

- macOS 與 Windows x86_64 是第一批 packaged install targets
- MLX 需要 Apple Silicon macOS
- Cloud provider server 不支援 request-time local adapter
- generated dataset splits 尚未彼此去重
- provider key set/remove 與 `doctor --fix` 仍是 CLI-only
- macOS signing 與 notarization 會留到後續 slice

版本功能與限制請看 [docs/user/version.md](../../../docs/user/version.md)。

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

## 參與合作

歡迎 issue、實驗、整合與 pull request。適合先做的方向包含文件、installer smoke test、平台 runtime notes、dataset 範例，以及使用 local HTTP daemon 的 client。

較大的修改前，請先讀 [AGENTS.md](../../../AGENTS.md) 與相關 [docs/contracts/](../../../docs/contracts/README.md)，並讓變更維持容易 review。

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

此專案採用 Apache License, Version 2.0。請見 [LICENSE](../../../LICENSE)。
