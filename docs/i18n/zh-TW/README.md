# Tentgent

Tentgent 是以 Rust 為主的本地 operator CLI，搭配 Python daemon 層管理模型 runtime、adapter、LoRA training、長時間運行的本地 server，以及本地 HTTP control plane。

目前 MVP 可以管理 provider key、下載並去重本地模型、匯入或下載 LoRA adapter、管理 dataset、執行單次聊天、訓練 LoRA adapter、提供本地 HTTP chat，並透過 daemon API expose 主要本地工作流程。

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
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.2.0/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.2.0/install.ps1 | iex
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

安裝、升級、固定版本與本機 package smoke test 請看 [docs/user/install.md](../../../docs/user/install.md)。

## 安裝後第一組指令

檢查本機 runtime：

```bash
tentgent doctor
tentgent status
```

設定 provider key 到系統 Keychain：

```bash
tentgent auth status
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

下載小型模型並執行單次聊天：

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent chat <model-ref> --message "user:Hello there"
```

單次 chat message 格式請看 [docs/user/commands.md](../../../docs/user/commands.md#chat)。

啟動 model-bound server：

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
curl -sS http://127.0.0.1:8780/healthz
```

model-bound server chat request 與 adapter rules 請看 [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md)。

啟動 daemon control plane：

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

開啟 terminal UI operator console：

```bash
tentgent tui
```

TUI 的 Operator mode 現在包含 `Chat` workspace，可透過既有 daemon
session/chat route 選擇 running server、建立或續接 session，並串流回覆；
預設只送出最近 2 則 persisted session messages 作為 context，composer
沒有 focus 時可按 `h` 在 `none` / `last 2` / `last 10` / `last 50` 間切換。
server/model lifecycle 與刪除/cleanup 類 mutation 仍保留在後續 slices。

完整 daemon API、endpoint、response shape、auth 與 error mapping 請看 [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md)。

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

## 目前版本

`v0.2.0` 擴充本地 HTTP daemon，讓 store、dataset、server、chat、training、diagnostics 與 bounded session workflow 都能透過 API 使用，並加入第一版 TUI setup surface。

`v0.1.4` 加入 `/v1/chat` 的 Server-Sent Events streaming，支援本地模型、本地相容 adapter，以及 OpenAI / Anthropic cloud provider server。

已包含：

- Hugging Face、OpenAI、Anthropic 的 provider auth key 管理
- content-addressed model、adapter、dataset stores
- OpenAI 與 Anthropic local server proxy runtimes
- dataset validation、prompt templates、multi-split provider synthesis、provider evaluation
- MLX、PEFT safetensors、llama-cpp GGUF 路徑的單次本地 chat
- 本地 HTTP daemon API，涵蓋 store、dataset、server、chat、training、diagnostics 與 bounded session workflows
- terminal UI status/settings surface，可做 daemon discovery、明確啟動 daemon、非秘密 config，以及受保護的本機 Keychain setup
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
