# Tentgent

Tentgent は Rust を主体とし、常駐する Python daemon レイヤーを組み合わせたローカル操作用 CLI です。モデルバックエンド、adapter、runtime 選択を扱います。

現在の MVP には provider auth key 管理と、内容ベースで重複排除するローカル model store が含まれています。

言語:
- 英語原文: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](../zh-TW/README.md)
- 日本語: [docs/i18n/ja/README.md](./README.md)

## リポジトリ構成

- `src/tentgent-core/`
  共通 Rust core 型、runtime contract、routing logic。
- `src/tentgent-cli/`
  Rust CLI エントリ crate。`tentgent` binary はここにあります。
- `src/tentgent-http/`
  Rust HTTP エントリ crate。
- `python/tentgent-daemon/`
  daemon 側 runtime 用の独立した Python サブプロジェクト。
- `python/tentgent-daemon/src/tentgent_daemon/`
  import 可能な Python package。runtime contract、backend adapter、CLI helper、internal tool を置きます。
- `docs/contracts/`
  Rust と Python の間の contract 文書。

## 命名

- product slug: `tentgent`
- binary name: `tentgent`
- service host: `agent.tentserv.com`
- app identifier: `com.tentserv.tentgent`
- environment variable prefix: `TENTGENT_`

## ドキュメント方針

- 最新情報は英語版 [README.md](../../../README.md) を参照してください。
- ルートの英語 README が source of truth です。
- 繁體中文版と日本語版は `docs/i18n/` に置き、英語版と同期します。

## Runtime Home

- CLI と将来の HTTP entry point は同じ daemon-managed runtime home を共有します。
- 既定の場所は固定 app identifier `com.tentserv.tentgent` から導出されます。
- 必要に応じて次の環境変数で上書きできます:
  - `TENTGENT_HOME`
  - `TENTGENT_MODELS_DIR`
  - `TENTGENT_ADAPTERS_DIR`
  - `TENTGENT_CACHE_DIR`
  - `TENTGENT_RUNTIME_DIR`
  - `TENTGENT_LOG_DIR`

## 開発フロー

- 日常の開発と手動テストは repo root で実行します。
- repo-local の runtime home は次を推奨します:
  - `TENTGENT_HOME="$PWD/.tentgent-test"`
- 長期的に残すテスト用ディレクトリは `.tentgent-test/` のみとし、実験用の一時 `.tentgent-*` は終了後に削除します。

## リポジトリ内テスト用コマンド

- Rust workspace をビルド:

```bash
cargo build --workspace
```

- CLI help を確認:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent --help
```

- 小さな Hugging Face モデルを取得:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

- 小さな MLX モデルを取得:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
```

- 小さな GGUF モデルを取得:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

- モデルの一覧・詳細・削除:

```bash
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model ls
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model inspect <short-ref>
TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model rm <hash>
```

- Python harness で単発チャット:

```bash
uv run --project python/tentgent-daemon tentgent-chat-once --model-ref <short-ref> --home "$PWD/.tentgent-test" --message "user:Hello there"
```

- Rust wrapper 経由のチャット:

```bash
./target/debug/tentgent chat <short-ref> --home "$PWD/.tentgent-test" --message "user:Hello there"
```

- `role:content` 形式では `system`、`user`、`assistant` を使えます。role を省略した場合は `user` として扱われます。

## Keychain プロンプト

- macOS では、保存済み provider secret を読む必要があるコマンドで Keychain prompt が表示されることがあります。
- `tentgent model ls` と `tentgent model inspect <REF>` は provider secret を読むべきではありません。
- 自分でビルドした binary を信頼しているなら、開発時に `Always Allow` を選ぶのは一般的に妥当です。
- 一回だけ上書きしたい場合は、次のように一時環境変数を使えます:

```bash
HF_TOKEN="your token" TENTGENT_HOME="$PWD/.tentgent-test" ./target/debug/tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
```

## 現在の状態

- Rust auth key flow は Hugging Face、OpenAI、Anthropic をサポートしています。
- model store MVP は `model add`、`model pull`、`model ls`、`model rm`、`model inspect` をサポートしています。
- `tentgent-chat-once` は次を実行できます:
  - `safetensors -> transformers`
  - `mlx -> mlx-lm`
  - `gguf -> llama.cpp`
- Rust `tentgent chat <MODEL_REF>` は Python chat harness を包み、対話プロンプトと `--stream` を維持します。
