# Tentgent

Tentgent 是一個以 Rust 為主、並搭配常駐 Python daemon 層的本地操作 CLI，用來管理模型後端、adapter，以及 runtime 選擇。

目前的 MVP 已包含 provider auth key 管理，以及具備內容去重能力的本地 model store。

語言：
- 英文原文： [README.md](../../../README.md)
- 繁體中文： [docs/i18n/zh-TW/README.md](./README.md)
- 日文： [docs/i18n/ja/README.md](../ja/README.md)

## 專案結構

- `src/tentgent-core/`
  共用 Rust core 型別、runtime contract 與 routing logic。
- `src/tentgent-cli/`
  Rust CLI 入口 crate，`tentgent` binary 位於此處。
- `src/tentgent-http/`
  Rust HTTP 入口 crate。
- `python/tentgent-daemon/`
  daemon 端 runtime 的獨立 Python 子專案，擁有自己的 `pyproject.toml`。
- `python/tentgent-daemon/src/tentgent_daemon/`
  可 import 的 Python package，放 runtime contract、backend adapter、CLI helper 與 internal tool。
- `docs/contracts/`
  Rust 與 Python 之間的 contract 文件。

## 命名

- product slug: `tentgent`
- binary name: `tentgent`
- service host: `agent.tentserv.com`
- app identifier: `com.tentserv.tentgent`
- environment variable prefix: `TENTGENT_`

## 文件規則

- 先讀英文版 [README.md](../../../README.md) 取得最新資訊。
- 根目錄英文 README 是 source of truth。
- 繁中與日文版本放在 `docs/i18n/` 下，應與英文版本同步。

## Runtime Home

- CLI 與未來的 HTTP entry point 應共用同一個 daemon-managed runtime home。
- 預設路徑應由固定 app identifier `com.tentserv.tentgent` 推導。
- 需要時可用環境變數覆蓋：
  - `TENTGENT_HOME`
  - `TENTGENT_MODELS_DIR`
  - `TENTGENT_ADAPTERS_DIR`
  - `TENTGENT_CACHE_DIR`
  - `TENTGENT_RUNTIME_DIR`
  - `TENTGENT_LOG_DIR`

## 開發流程

- 日常開發與手動測試應在 repo root 執行。
- 建議使用固定的 repo-local runtime home：
  - `TENTGENT_HOME="$PWD/.tentgent-test"`
- 只保留 `.tentgent-test/` 作為長期測試資料夾，其他臨時 `.tentgent-*` 目錄在實驗結束後應刪除。
- 若未設定環境變數，已安裝的 binary 會回退到平台預設 runtime home。

## Repo 內測試指令

- 建置 Rust workspace：

```bash
cargo build --workspace
```

- 查看 CLI help：

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent --help
```

- 拉一個小型 Hugging Face 模型：

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

- 拉一個小型 MLX 模型：

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
```

- 拉一個小型 GGUF 模型：

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

- 列出、檢查、刪除模型：

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model ls
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model inspect <short-ref>
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model rm <hash>
```

- 用 Python harness 單次聊天：

```bash
uv run --project python/tentgent-daemon tentgent-chat-once --model-ref <short-ref> --home "$PWD/.tentgent-test" --message "user:Hello there"
```

- 走 Rust wrapper：

```bash
./target/debug/tentgent chat <short-ref> --home "$PWD/.tentgent-test" --message "user:Hello there"
```

- `role:content` 支援 `system`、`user`、`assistant`；若未指定 role，預設視為 `user`。

## Keychain 提示

- 在 macOS 上，若命令需要讀取已儲存的 provider secret，Tentgent 可能觸發 Keychain prompt。
- `tentgent model ls` 與 `tentgent model inspect <REF>` 不應讀取 provider secret。
- 若你信任本機自行建置的 binary，開發時選 `Always Allow` 通常是合理流程。
- 若只想覆蓋單次執行，可直接用一次性的環境變數，例如：

```bash
HF_TOKEN="your token" TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

## 目前狀態

- Rust auth key flow 已支援 Hugging Face、OpenAI、Anthropic。
- model store MVP 已支援 `model add`、`model pull`、`model ls`、`model rm`、`model inspect`。
- `tentgent-chat-once` 已可跑：
  - `safetensors -> transformers`
  - `mlx -> mlx-lm`
  - `gguf -> llama.cpp`
- Rust `tentgent chat <MODEL_REF>` 已可包 Python chat harness，並保留互動 prompt 與 `--stream`。
