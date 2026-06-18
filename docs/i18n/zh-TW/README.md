# Tentgent

Tentgent 是本地 AI workflow operator：Rust CLI 搭配本地 daemon REST API，
管理模型 runtime、adapter、dataset、LoRA training、長時間運行的本地
server，以及短期工作 session。

目前產品介面是 CLI 加 daemon REST；沒有 terminal UI 指令。

## 語言與文件

- 英文 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](./README.md)
- 日文: [docs/i18n/ja/README.md](../ja/README.md)
- 完整英文使用文件: [docs/user/README.md](../../../docs/user/README.md)
- HTTP API reference: [docs/user/api.md](../../../docs/user/api.md)
- 小型模型 fixture 與 smoke test: [docs/user/model-fixtures.md](../../../docs/user/model-fixtures.md)
- 模型支援目錄與 support status: [docs/user/model-support-catalog.md](../../../docs/user/model-support-catalog.md)
- 開發者文件: [docs/development/README.md](../../../docs/development/README.md)

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
tentgent model catalog --capability chat --publisher Qwen
tentgent model pull google/gemma-3-1b-it
tentgent model ls
tentgent chat <model-ref> --message "user:Hello"
tentgent daemon start --host 127.0.0.1 --port 8790
```

## 安裝工具

建議 macOS 使用者透過 project Homebrew tap 安裝：

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
tentgent --version
```

建議 Windows PowerShell 使用者從最新 GitHub Release 安裝：

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

Linux x86_64 可從最新 GitHub Release 安裝：

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.sh | bash
tentgent doctor
```

Linux preview 使用 GitHub Release tarball 與預設 `base` runtime bootstrap
profile。Linux 上尚未宣告完整 managed runtime 與 local model backend parity。
如果你希望 runtime data 不放在預設 direct-installer support directory 裡，
請在 bootstrap 前設定並持久化 `TENTGENT_HOME`。

若你想要可重現的 script-based 安裝，請使用 GitHub Release installer：

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.ps1 | iex
```

如果你之前用 `install.sh` 安裝過，`~/.local/bin/tentgent` 可能會在
`PATH` 中蓋過 Homebrew 版。可直接檢查 Homebrew binary：

```bash
/opt/homebrew/opt/tentgent/bin/tentgent -V
```

Homebrew 升級使用 `brew upgrade hiroliang/tap/tentgent`。`TENTGENT_HOME`
下的使用者 runtime data 會被保留。

安裝、升級、固定版本、本機 package smoke test 與移除說明請看 [docs/user/install.md](../../../docs/user/install.md)。

## 設定 Key

檢查本機 runtime 與 provider key 狀態：

```bash
tentgent doctor
tentgent runtime status
tentgent auth status
tentgent auth mode
```

設定 provider key 到系統 Keychain：

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
tentgent auth gemini set
```

也可以用環境變數或目前 process 讀取的 `.env`：

```bash
cat > .env <<'EOF'
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GEMINI_API_KEY=...
EOF
```

可以針對 provider 設定 Tentgent 要從哪裡讀 key：

```bash
tentgent auth mode openai auto
tentgent auth mode openai env
tentgent auth mode gemini file --path ~/.config/tentgent/provider.env
tentgent auth mode anthropic none
```

`auto` 是預設，依序使用 request/prompt、`.env` / process env、process
cache、Keychain。OpenShell 或其他 launcher 只注入標準環境變數時，使用
`env`。`file` 僅讀取明確指定的 env file；`none` 會停用該 provider 的本機
secret resolution。

Auth file 使用 dotenv-style provider 變數：

```dotenv
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GEMINI_API_KEY=...
```

provider secret resolution 與 Keychain boundaries 請看 [docs/contracts/auth-secrets.md](../../../docs/contracts/auth-secrets.md)。

## 匯入、拉取、移除模型

管理模型：

```bash
tentgent model catalog --capability chat --publisher Qwen
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent model ls
tentgent model inspect <model-ref-or-prefix>
tentgent model add /absolute/path/to/model
tentgent model rm <model-ref>
```

`model catalog` 可先瀏覽內建模型家族與 support hints。`model ls` 顯示精簡
support status，`model inspect` 顯示每個 capability 的 proof、hint、backend
與 reason 細節。

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
tentgent server inspect <server-ref>
tentgent server ps
tentgent server stop <server-ref>
```

Direct model-server chat 是 stateless。若要 model-ref based native 或 compatible chat routes，請走 daemon。server chat request 與 adapter rules 請看 [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md)。
Local model-bound server 在 `server ls` 會顯示精簡 model short ref，完整
model_ref 與 selected-capability support status 請看 `server inspect`。

## 起停 Daemon

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

需要 daemon-native、OpenAI-compatible、Claude-compatible、Gemini-compatible chat routes 時：

```bash
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"model_ref":"<model-ref>","messages":[{"role":"user","content":"Hello"}],"stream":false}'

curl -sS http://127.0.0.1:8790/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"<model-ref>","messages":[{"role":"user","content":"Hello"}],"stream":true}'
```

停止 daemon：

```bash
tentgent daemon stop
```

完整 daemon API、endpoint、response shape、auth 與 error mapping 請看 [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md)。

## 移除工具

Homebrew 移除只會移除工具本體與 support files，不刪使用者 runtime data：

```bash
brew uninstall hiroliang/tap/tentgent
```

若是直接用 `install.sh` 安裝，移除這些檔案：

```bash
rm -f "$HOME/.local/bin/tentgent"
rm -rf "$HOME/.local/share/tentgent"
```

Linux preview 安裝時，`$HOME/.local/share/tentgent` 也可能是預設
runtime home。除非你確定要刪 runtime data，或已用 `TENTGENT_HOME`
把 runtime data 放到其他位置，否則不要刪這個目錄。

可選擇清除 safe-to-recreate bootstrap cache：

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

除非你確定要刪掉 models、adapters、datasets、sessions、servers、train records 與其他本地 runtime data，否則不要刪 `TENTGENT_HOME`。移除與 runtime-home 細節請看 [docs/user/install.md](../../../docs/user/install.md) 與 [docs/user/runtime.md](../../../docs/user/runtime.md)。

## 版本說明

`v0.7.0` 是 support status release，讓模型支援狀態可以被 `model ls`、
`model inspect`、`server inspect` 與 `doctor` 檢視。這版會顯示
`verified`、`supported`、`failed`、`unsupported`、`unknown`、`stale`
等狀態與下一步方向，但尚未把 support status 變成硬性 runtime gate。

`v0.6.0` 是 compatibility contract release，明確記錄 OpenAI、Claude /
Anthropic、Gemini-compatible API 子集合與 unsupported request 的穩定錯誤。

`v0.3.5-alpha.0` 是 CLI plus daemon REST consolidation release，移除舊
terminal UI、legacy core 與 legacy HTTP crates，並把 broad diagnostics
收斂到 `doctor`。

`v0.3.4-alpha.2` 是 Linux x86_64 preview release，包含 release tarball
install、預設 base runtime bootstrap，以及 Ubuntu 24.04 Docker smoke 過的
`doctor` readiness。

`v0.3.3` 加入 Homebrew tap update tooling，讓 stable release 後的 formula URL 與 checksum 更新更可重複。

`v0.3.2` 加入 `tentgent runtime bootstrap`，作為 Homebrew / package-manager 安裝後準備 managed Python runtime 的公開入口。

`v0.3.1` 是 0.3.x stable hotfix，補上 macOS release binary ad-hoc signing 與 installer 安裝後的 quarantine cleanup，降低下載安裝後被 macOS 直接 kill 的情況。

版本功能與限制請看 [docs/user/version.md](../../../docs/user/version.md)。

## 完整 CLI 指令文件

README 只保留最短入口。完整 CLI 指令文件請看 [docs/user/commands.md](../../../docs/user/commands.md)，涵蓋 auth、models、adapters、datasets、chat、servers、daemon、sessions 與 LoRA training。

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
export TENTGENT_PYTHON_DIR="$PWD/python/tentgent-model-runtime"
export TENTGENT_PYTHON_ENV_DIR="$PWD/python/tentgent-model-runtime/.venv"
```

Tentgent 會先讀 `.env` / env，再 fallback 到系統 Keychain。若要讓 `.env`
行為可預期，請從含有 `.env` 的目錄執行 `tentgent`，或直接在 shell export
變數。

更多 runtime home、Python runtime 與 Keychain prompt 說明請看 [docs/user/runtime.md](../../../docs/user/runtime.md)。

## 目前能力

- Hugging Face、OpenAI、Anthropic、Gemini 的 provider auth key 管理
- content-addressed model、adapter、dataset stores
- OpenAI、Anthropic 與 Gemini cloud provider server runtimes
- local model-bound server runtimes for chat、embedding、rerank、audio、vision、video 與 image endpoint families
- 內建 model support catalog、support status、local proof 與 `doctor` diagnostics
- dataset validation、prompt templates、multi-split provider synthesis、provider evaluation
- MLX、PEFT safetensors、llama-cpp GGUF 路徑的單次本地 chat
- 本地 HTTP daemon API，涵蓋 store、dataset、server、chat、training、diagnostics 與 bounded session workflows
- managed LoRA train plans、durable run records、metrics/log inspection，以及可執行的 MLX / PEFT training loops
- bounded transcript compaction 的本地 session，作為短期 working context
- 一般 installer 安裝用的 Python runtime bootstrap，以及 package-manager 安裝用的 `tentgent runtime bootstrap`

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
