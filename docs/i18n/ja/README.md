# Tentgent

Tentgent は Rust を主体としたローカル operator CLI で、Python daemon レイヤーを使って model runtime、adapter、長時間動作するローカル server を管理します。

現在の MVP は provider key の管理、ローカル model の取得と重複排除、LoRA adapter の import / pull、単発 chat、非 streaming のローカル HTTP chat に対応しています。

## 言語

- 英語 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](../zh-TW/README.md)
- 日本語: [docs/i18n/ja/README.md](./README.md)

## インストール状況

Tentgent は現在 source-first です。packaged Python runtime bootstrap はまだ整理中のため、一般ユーザー向け release-ready ではありません。

現在利用可能：
- この repository から build して実行
- `scripts/package-local.sh` で local release-like smoke-test tarball を作成
- `uv` がすでに利用できる開発環境では `tentgent doctor --fix` を developer bootstrap として使用

今後の予定：
- ユーザーに `uv` の事前インストールを求めない installer-managed Python bootstrap
- Homebrew install
- packaged app または daemon distribution
- 非開発者向けの簡単な bootstrap commands

packaged installer が用意されるまでは、checkout 済み repository から Tentgent を実行してください。

local release-like smoke-test artifact を作成:

```bash
scripts/package-local.sh
```

この script は `dist/tentgent-<version>-<target>.tar.gz` と `dist/checksums.txt` を書き込みます。
この artifact は install layout のテストには有用ですが、installer が Python runtime bootstrap を自前で処理し、事前インストール済み `uv` に依存しなくなるまでは、一般ユーザー向け release として公開すべきではありません。

重い Python ML dependencies をダウンロードせずに installer layout だけを smoke-test する場合:

```bash
scripts/install.sh \
  --archive dist/tentgent-0.1.0-aarch64-apple-darwin.tar.gz \
  --checksums dist/checksums.txt \
  --prefix /tmp/tentgent-install-smoke \
  --skip-python-bootstrap
```

`--skip-python-bootstrap` を外すと full managed Python bootstrap を実行します。この path は pinned `uv` を Tentgent bootstrap cache にダウンロードし、`TENTGENT_HOME/runtime/python-env` を作成し、Python ML dependencies をインストールします。

## Quick Start

Rust workspace を build:

```bash
cargo build --workspace
```

local runtime の health check を実行:

```bash
./target/debug/tentgent doctor
```

開発時に `uv` が利用できる場合は managed Python environment を作成または同期:

```bash
./target/debug/tentgent doctor --fix
```

テスト中は repository-local runtime home を使います:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

小さな model を取得:

```bash
./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

保存済み model を一覧表示:

```bash
./target/debug/tentgent model ls
```

単発 chat を実行:

```bash
./target/debug/tentgent chat <model-ref> --message "user:Hello there"
```

長時間動作するローカル server を起動:

```bash
./target/debug/tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

別の terminal から server を呼び出す:

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

background server mode を使う場合は `--detach` を追加し、次のコマンドで process を管理します:

```bash
./target/debug/tentgent server ls
./target/debug/tentgent server ps
./target/debug/tentgent server stop <server-ref>
```

## よく使う操作

provider key を設定:

```bash
./target/debug/tentgent auth hf set
./target/debug/tentgent auth openai set
./target/debug/tentgent auth anthropic set
```

Hugging Face から model を取得:

```bash
./target/debug/tentgent model pull google/gemma-3-1b-it
./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

adapter を import または pull:

```bash
./target/debug/tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
./target/debug/tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
./target/debug/tentgent adapter ls
```

将来の training / evaluation 用にローカル dataset を import:

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

将来 tuning に使う dataset directory は、少なくとも `train.jsonl` を含む場合に ready として扱います。`valid.jsonl`、`test.jsonl`、`eval_cases.jsonl`、source `manifest.json` は任意の metadata または evaluation 用 companion file です。
新しい chat および tool-use dataset は、[docs/contracts/dataset-schema.md](../../contracts/dataset-schema.md) の canonical `tentgent.chat.v1` schema を使用してください。

管理済み dataset を編集したい場合は、まず working directory に export し、そこで編集してから `dataset add` を再実行して新しい content-derived dataset reference を作成します。
`dataset rm` は managed store record と index のみを削除し、export 済みの working copy は削除しません。

managed LoRA training plan を作成、確認、実行:

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

Tentgent は model format から backend を自動選択します。`mlx` model は MLX、`safetensors` model は PEFT、`gguf` model は LoRA training では blocked です。Plan は永続化される recipe です。`--review` は生成された設定を preview し、保存前に確認します。`--interactive` は review 前に一般的な設定を編集できます。`run` は durable run records を作成し、成功した MLX または PEFT output を managed adapter として import します。`plan rm` は stored plan とその run records のみを削除します。

よく使う plan override: `--rank` は adapter 容量を決めます。`--learning-rate`、`--batch-size`、`--grad-accum`、`--max-steps`、`--seed` は optimization を制御します。`--max-seq-length` は token 長を制限します。
Chat 系 dataset では `--mask-prompt` を使うと、system/user/tool context はモデルに見せつつ、loss は assistant output のみに適用できます。
MLX 専用: `--num-layers` は tuning 対象 layer 数を制限し、`--grad-checkpoint` は速度と引き換えに memory 使用量を下げます。
PEFT 専用: `--load-in-4bit` と `--load-in-8bit` は将来の quantized loading 用 flag です。現在の minimal PEFT loop では拒否されます。

adapter 付きで単発 chat を実行:

```bash
./target/debug/tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Think step by step: what is 12 * 7?" \
  --max-tokens 128
```

ローカルの `model_ref`、`adapter_ref`、`dataset_ref`、`server_ref` が必要な場所では、完全な reference または一意な short prefix を使えます。

## LoRA Server Smoke Test

管理済み model の server を起動:

```bash
./target/debug/tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

Base request:

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

Adapter request:

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

期待される signal: adapter request は回答スタイルを明確に変えるはずです。ローカル Gemma 3 1B IT smoke test では、base model は短く答えた後に noisy になり、LoRA request は構造化された step-by-step の計算を出力して `84` に到達しました。

## Runtime Home

Tentgent は通常、runtime state を source code の外に保存します。開発時は次を推奨します:

```bash
export TENTGENT_HOME="$PWD/.tentgent-test"
```

Runtime directories:

- `models/`
- `adapters/`
- `datasets/`
- `train/`
- `servers/`
- `cache/`
- `runtime/`
- `logs/`

対応する path override:

- `TENTGENT_HOME`
- `TENTGENT_MODELS_DIR`
- `TENTGENT_ADAPTERS_DIR`
- `TENTGENT_DATASETS_DIR`
- `TENTGENT_TRAIN_DIR`
- `TENTGENT_CACHE_DIR`
- `TENTGENT_RUNTIME_DIR`
- `TENTGENT_LOG_DIR`

環境変数は process 起動時に読み込まれます。Tentgent は shell 環境設定を書き戻したり永続化したりしません。

## Backend 状態

- `tentgent doctor` は installation と runtime の health check を実行します。
- `tentgent doctor --fix` は developer-only bootstrap であり、現時点では `uv` が必要です。
- `tentgent status` は現在の platform と backend capability 状態を表示します。
- `safetensors` model は Python dependencies が入っている場合に `transformers-peft` backend で実行されます。
- `mlx` model は Apple Silicon macOS 上でのみ MLX backend で実行されます。
- `gguf` model は `llama-cpp-python` dependency が入っている場合に実行されます。
- PEFT LoRA adapter は request ごとに `adapter_ref` で指定できます。
- MLX adapter も request ごとに指定できます。正しさを優先し、adapter を切り替えると MLX model を reload します。
- `llama-cpp` の外部 adapter execution はこの MVP では未実装です。
- HTTP `/v1/chat` は non-streaming です。`stream=true` は現在 `501` を返します。
- Windows は計画中ですが、まだ fully supported release target ではありません。MLX は Windows では blocked されます。

## Keychain プロンプト

macOS では、保存済み provider secret が必要で、環境変数による override がない場合、Tentgent が Keychain prompt を表示することがあります。

よくあるコマンド:

- `tentgent auth hf`
- `tentgent auth openai`
- `tentgent auth anthropic`
- `tentgent model pull <HF_REPO>`
- `tentgent adapter pull <HF_REPO>`

ローカルで build した `./target/debug/tentgent` binary を信頼しているなら、開発時に `Always Allow` を選ぶのは妥当です。未署名の開発 binary を再 build したり移動したりすると、macOS が再度確認することがあります。

一回だけ Keychain 読み取りを避けたい場合は、一時的な環境変数を渡します:

```bash
HF_TOKEN="your token" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

一時的な環境変数はそのコマンドだけに適用され、`unset` は不要です。

## プロジェクト文書

- repository context と documentation routing は [AGENTS.md](../../../AGENTS.md) を参照してください。
- agent workflow と role boundary は [CLAUDE.md](../../../CLAUDE.md) を参照してください。
- 開発者向け command と repository-local test は [docs/development/README.md](../../development/README.md) を参照してください。
- cross-language interface は [docs/contracts/](../../contracts/README.md) を参照してください。
- active staged plans は [docs/plans/](../../plans/README.md) を参照してください。
- historical context が必要な場合のみ [docs/plans/archive/](../../plans/archive/README.md) を参照してください。

## License

このプロジェクトは proprietary software であり、all rights reserved です。詳しくは [LICENSE](../../../LICENSE) を参照してください。

## Repository 構成

- `src/tentgent-core/`
  共通 Rust core types、runtime contracts、routing logic。
- `src/tentgent-cli/`
  Rust CLI entry crate。`tentgent` binary はここにあります。
- `src/tentgent-http/`
  Rust HTTP entry crate。
- `python/tentgent-daemon/`
  daemon runtime 作業用の Python subproject。
- `python/tentgent-daemon/src/tentgent_daemon/`
  import 可能な Python package。runtime contracts、backend adapters、CLI helpers、internal tools を含みます。
- `docs/contracts/`
  Rust entry points、shared core logic、Python daemon 間の interface documents。
- `Makefile`
  formatting、checking、building、Rust workspace 実行用の root developer shortcuts。

## 命名

- product slug: `tentgent`
- binary name: `tentgent`
- service host: `agent.tentserv.com`
- app identifier: `com.tentserv.tentgent`
- environment variable prefix: `TENTGENT_`
