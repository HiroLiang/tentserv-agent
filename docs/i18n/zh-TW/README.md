# Tentgent

Tentgent 是以 Rust 為主的本地 operator CLI，搭配 Python daemon 層管理模型 runtime、adapter，以及長時間運行的本地 server。

目前 MVP 可以管理 provider key、下載並去重本地模型、匯入或下載 LoRA adapter、執行單次聊天，以及提供非 streaming 的本地 HTTP chat。

## 語言

- 英文 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](./README.md)
- 日文: [docs/i18n/ja/README.md](../ja/README.md)

## 安裝狀態

Tentgent 目前仍是 source-first。

目前可用：
- 從此 repository build 並執行

之後規劃：
- Homebrew 安裝
- packaged app 或 daemon distribution
- 給非開發者使用的簡化 bootstrap 指令

在 packaged installer 完成前，請從已 checkout 的 repository 執行 Tentgent。

## 快速開始

建置 Rust workspace：

```bash
cargo build --workspace
```

測試時使用 repo-local runtime home：

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

下載小型模型：

```bash
./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

列出已儲存模型：

```bash
./target/debug/tentgent model ls
```

執行單次聊天：

```bash
./target/debug/tentgent chat <model-ref> --message "user:Hello there"
```

啟動長時間運行的本地 server：

```bash
./target/debug/tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

在另一個 terminal 呼叫 server：

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Hello there"}
    ],
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

若要背景執行 server，加上 `--detach`，並用以下指令管理 process：

```bash
./target/debug/tentgent server ls
./target/debug/tentgent server ps
./target/debug/tentgent server stop <server-ref>
```

## 常用任務

設定 provider key：

```bash
./target/debug/tentgent auth hf set
./target/debug/tentgent auth openai set
./target/debug/tentgent auth anthropic set
```

從 Hugging Face 下載模型：

```bash
./target/debug/tentgent model pull google/gemma-3-1b-it
./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

匯入或下載 adapter：

```bash
./target/debug/tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
./target/debug/tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
./target/debug/tentgent adapter ls
```

匯入本機 dataset，之後可用於訓練或評估：

```bash
./target/debug/tentgent dataset add /path/to/dataset.jsonl
./target/debug/tentgent dataset add /path/to/dataset-dir
./target/debug/tentgent dataset ls
./target/debug/tentgent dataset inspect <dataset-ref>
./target/debug/tentgent dataset export <dataset-ref> /path/to/work-dir
./target/debug/tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
./target/debug/tentgent dataset diff <dataset-ref> --path /path/to/work-dir
./target/debug/tentgent dataset rm <dataset-ref>
```

之後若要拿來 tuning，dataset directory 至少要有 `train.jsonl` 才會被視為 ready。`valid.jsonl`、`test.jsonl`、`eval_cases.jsonl`、source `manifest.json` 都是可選的 metadata 或評估搭配檔。
新的 chat 與 tool-use dataset 應使用 [docs/contracts/dataset-schema.md](../../contracts/dataset-schema.md) 中的 canonical `tentgent.chat.v1` schema。

若要修改已管理的 dataset，先 export 到工作資料夾，在那邊修改後再重新執行 `dataset add`，產生新的 content-derived dataset reference。
`dataset rm` 只會刪 managed store record 與 index，不會刪 export 出去的 working copy。

建立、檢查，並執行 managed LoRA training plan：

```bash
./target/debug/tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
./target/debug/tentgent train lora plan ls
./target/debug/tentgent train lora plan inspect <plan-ref>
./target/debug/tentgent train lora plan rm <plan-ref>
./target/debug/tentgent train lora run <plan-ref>
```

Tentgent 會根據 model format 自動選 backend：`mlx` 模型使用 MLX，`safetensors` 模型使用 PEFT，`gguf` 模型目前不支援 LoRA training。Plan 是可持久化的 recipe；`--review` 會先預覽設定並詢問是否儲存，`--interactive` 則可在 review 前調整常用設定。`run` 會建立 durable run records，並把成功的 MLX 或 PEFT output 匯入 managed adapter。`plan rm` 只會刪除 stored plan 與其 run records。

常用 plan override：`--rank` 控制 adapter 容量；`--learning-rate`、`--batch-size`、`--grad-accum`、`--max-steps`、`--seed` 控制訓練最佳化；`--max-seq-length` 限制 token 長度。
Chat 類 dataset 可使用 `--mask-prompt`，讓模型看得到 system/user/tool context，但 loss 只計算 assistant output。
MLX 專用：`--num-layers` 限制要調整的層數，`--grad-checkpoint` 用較慢速度換較低記憶體使用。
PEFT 專用：`--load-in-4bit` 與 `--load-in-8bit` 是預留的量化載入旗標；目前 minimal PEFT loop 會拒絕使用。

用 adapter 執行單次聊天：

```bash
./target/debug/tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Think step by step: what is 12 * 7?" \
  --max-tokens 128
```

凡是需要本地 `model_ref`、`adapter_ref`、`dataset_ref` 或 `server_ref` 的地方，都可使用完整 reference 或唯一 short prefix。

## LoRA Server Smoke Test

替已管理的模型啟動 server：

```bash
./target/debug/tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

Base request：

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Think step by step: what is 12 * 7?"}
    ],
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

Adapter request：

```bash
curl -s http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "messages": [
      {"role": "user", "content": "Think step by step: what is 12 * 7?"}
    ],
    "adapter_ref": "<adapter-ref>",
    "max_tokens": 128,
    "temperature": 0.0
  }'
```

預期訊號：adapter request 應該明顯改變回答風格。在本地 Gemma 3 1B IT smoke test 中，base model 回答較短且後段變 noisy；LoRA request 則輸出結構化 step-by-step 計算並得到 `84`。

## Runtime Home

Tentgent 預設會把 runtime state 放在 source code 之外。開發時建議使用：

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Runtime 目錄包含：

- `models/`
- `adapters/`
- `datasets/`
- `train/`
- `servers/`
- `cache/`
- `runtime/`
- `logs/`

支援的路徑覆蓋：

- `TENTGENT_HOME`
- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`
- `TENTGENT_TRAIN_DIR`
- `TENTGENT_CACHE_DIR`
- `TENTGENT_RUNTIME_DIR`
- `TENTGENT_LOG_DIR`

環境變數會在 process 啟動時讀取。Tentgent 不會回寫或持久化 shell 環境設定。

## Backend 狀態

- `safetensors` 模型透過 `transformers-peft` backend 執行。
- `mlx` 模型在 Apple Silicon 上透過 MLX backend 執行。
- `gguf` 模型透過 `llama-cpp-python` 執行。
- PEFT LoRA adapter 可透過 `adapter_ref` 逐 request 指定。
- MLX adapter 可逐 request 指定；為了正確性，切換 adapter 時會重載 MLX 模型。
- `llama-cpp` 外部 adapter 執行尚未在此 MVP 實作。
- HTTP `/v1/chat` 目前是非 streaming；`stream=true` 會回 `501`。

## Keychain 提示

在 macOS 上，若命令需要讀取已儲存的 provider secret，且沒有環境變數覆蓋，Tentgent 可能會觸發 Keychain prompt。

常見命令包含：

- `tentgent auth hf`
- `tentgent auth openai`
- `tentgent auth anthropic`
- `tentgent model pull <HF_REPO>`
- `tentgent adapter pull <HF_REPO>`

若你信任本機自行建置的 `./target/debug/tentgent` binary，開發時選擇 `Always Allow` 是合理流程。重新 build 或移動未簽章的開發 binary 可能讓 macOS 再次詢問。

若只想跳過單次 Keychain 讀取，可使用一次性的環境變數：

```bash
HF_TOKEN="your token" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

一次性環境變數只影響該命令，不需要 `unset`。

## 專案文件

- 先讀 [AGENTS.md](../../../AGENTS.md) 了解 repository context 與文件路由。
- 再讀 [CLAUDE.md](../../../CLAUDE.md) 了解 agent workflow 與 role 邊界。
- 使用 [docs/development/README.md](../../development/README.md) 查開發者命令與 repo-local 測試方式。
- 使用 [docs/contracts/](../../contracts/README.md) 查跨語言介面。
- 使用 [docs/plans/](../../plans/README.md) 查 active staged plans。
- 只在需要歷史脈絡時查 [docs/plans/archive/](../../plans/archive/README.md)。

## 授權

本專案為 proprietary software，保留所有權利。請見 [LICENSE](../../../LICENSE)。

## Repository 結構

- `src/tentgent-core/`
  共用 Rust core types、runtime contracts 與 routing logic。
- `src/tentgent-cli/`
  Rust CLI entry crate。`tentgent` binary 位於此處。
- `src/tentgent-http/`
  Rust HTTP entry crate。
- `python/tentgent-daemon/`
  daemon runtime 工作用的 Python 子專案。
- `python/tentgent-daemon/src/tentgent_daemon/`
  可 import 的 Python package，包含 runtime contracts、backend adapters、CLI helpers 與 internal tools。
- `docs/contracts/`
  Rust entry points、shared core logic 與 Python daemon 之間的 interface documents。
- `Makefile`
  root developer shortcuts，用於 formatting、checking、building 與 running Rust workspace。

## 命名

- product slug: `tentgent`
- binary name: `tentgent`
- service host: `agent.tentserv.com`
- app identifier: `com.tentserv.tentgent`
- environment variable prefix: `TENTGENT_`
