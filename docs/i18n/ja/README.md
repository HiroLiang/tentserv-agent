# Tentgent

Tentgent は Rust を主体としたローカル operator CLI で、Python daemon レイヤーを使って model runtime、adapter、LoRA training、長時間動作するローカル server、ローカル HTTP control plane を管理します。

現在の MVP は provider key の管理、ローカル model の取得と重複排除、LoRA adapter の import / pull、dataset 管理、単発 chat、LoRA adapter training、ローカル HTTP chat、主要なローカル workflow の daemon API に対応しています。

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
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.0-alpha.1/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.0-alpha.1/install.ps1 | iex
```

その後、デフォルトのインストール先を `PATH` に追加し、runtime を確認します:

```bash
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) export PATH="$HOME/.local/bin:$PATH" ;;
esac
tentgent doctor
tentgent --version
```

upgrade は installer を再実行します。`TENTGENT_HOME` 配下の user runtime data は保持されます。

install、upgrade、pinned version、local package smoke test は [docs/user/install.md](../../../docs/user/install.md) を参照してください。

## インストール後の最初のコマンド

ローカル runtime を確認:

```bash
tentgent doctor
tentgent status
```

provider key を system Keychain に保存:

```bash
tentgent auth status
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
```

環境変数または現在の process が読む `.env` も使えます:

```bash
cat > .env <<'EOF'
HF_TOKEN=...
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
EOF
```

小さな model を取得して単発 chat を実行:

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent chat <model-ref> --message "user:Hello there"
```

単発 chat の message format は [docs/user/commands.md](../../../docs/user/commands.md#chat) を参照してください。

model-bound server を起動:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
curl -sS http://127.0.0.1:8780/healthz
```

model-bound server chat request と adapter rules は [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md) を参照してください。

daemon control plane を起動:

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

terminal UI operator console を開く:

```bash
tentgent tui
```

TUI の Operator mode には `Chat` workspace があり、既存の daemon
session/chat route で running server を選び、session を作成または再開して
streaming 応答を表示できます。デフォルトでは直近 2 件の persisted
session messages だけを context として送り、composer 以外に focus がある時は
`h` で `none` / `last 2` / `last 10` / `last 50` を切り替えます。
`Models`、`Adapters`、`Datasets` では `a` で guarded store/dataset actions を
開き、既存の daemon HTTP routes で pull/import/bind/validate/template/export/
diff/synth/eval/remove を実行できます。remove は selected short ref または full
ref の入力確認が必要です。`Servers` と `Training` でも guarded runtime
actions を使い、既存 daemon routes で server spec の
create/start/stop/remove、LoRA plan の preview/create/remove、LoRA run の
start と bounded metrics/log tail monitoring ができます。server start は
background job ではなく、training runs も Jobs registry へ mirror しません。
TUI は fake cancel を表示しません。session delete/compact、cleanup 系の
mutation は後続 slices に残します。
長時間の pull/import/synth/eval は daemon-side background jobs として動き、
footer、Dashboard、`Jobs` 画面に進捗を表示します。既存の同期 HTTP routes は
従来の response shape を維持します。Slice 4.1 では cancel route はありません。

完整な daemon API、endpoints、response shapes、auth、error mapping は [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md) を参照してください。

## API と Contracts

詳細な contract は [docs/contracts/](../../../docs/contracts/README.md) に分割し、README は入口として読みやすく保ちます。

- [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md)
  完整な local daemon API contract、endpoints、auth、response shape、error mapping。
- [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md)
  model-bound server chat request shape と adapter validation rules。
- [docs/contracts/session-store.md](../../../docs/contracts/session-store.md)
  session metadata、message records、mutation rules、bounded compaction。
- [docs/contracts/runtime-home.md](../../../docs/contracts/runtime-home.md)
  runtime home、store path、Python runtime、environment override rules。
- [docs/contracts/auth-secrets.md](../../../docs/contracts/auth-secrets.md)
  provider secret resolution、`.env` / env behavior、Keychain boundaries。
- [docs/contracts/training-lora.md](../../../docs/contracts/training-lora.md)
  managed LoRA plan と run boundaries。

## Paths と `.env`

通常の runtime state を移動するには `TENTGENT_HOME` を設定します:

```bash
export TENTGENT_HOME="$HOME/.tentgent"
```

特定の store や Python runtime だけを移動することもできます:

```bash
export TENTGENT_MODELS_DIR="/Volumes/models/tentgent"
export TENTGENT_DATASETS_DIR="$HOME/datasets/tentgent"
export TENTGENT_PYTHON_DIR="$PWD/python/tentgent-daemon"
export TENTGENT_PYTHON_ENV_DIR="$PWD/python/tentgent-daemon/.venv"
```

Tentgent は `.env` / env を先に読み、なければ system Keychain に fallback します。
`.env` の挙動を予測しやすくするには、その `.env` がある directory から
`tentgent` を実行するか、shell で明示的に export してください。

runtime home、Python runtime、Keychain prompt の詳細は [docs/user/runtime.md](../../../docs/user/runtime.md) を参照してください。

## 現在のバージョン

`v0.3.0-alpha.1` は TUI preview release です。chat、jobs、resources、store actions、server/training actions、picker-based create flows、session delete、compact ref display を追加します。TUI interaction model はまだ調整中の alpha です。

`v0.2.0` はローカル HTTP daemon を拡張し、store、dataset、server、chat、training、diagnostics、bounded session workflow を API から利用できるようにし、最初の TUI setup surface も追加します。

`v0.1.4` は `/v1/chat` の Server-Sent Events streaming を追加し、local model、compatible local adapter、OpenAI / Anthropic cloud provider server に対応します。

含まれる機能:

- Hugging Face、OpenAI、Anthropic の provider auth key 管理
- content-addressed model、adapter、dataset stores
- OpenAI と Anthropic の local server proxy runtimes
- dataset validation、prompt templates、multi-split provider synthesis、provider evaluation
- MLX、PEFT safetensors、llama-cpp GGUF path の one-shot local chat
- store、dataset、server、chat、training、diagnostics、bounded session workflow を扱う local HTTP daemon API
- daemon discovery、chat、jobs、resources、store/server/training actions、session cleanup、guarded local setup のための terminal UI operator console
- managed LoRA train plans、durable run records、metrics/log inspection、実行可能な MLX / PEFT training loops
- bounded transcript compaction を備えた local sessions
- 通常 install 用の installer-managed Python runtime bootstrap

現在の制限:

- macOS と Windows x86_64 が最初の packaged install targets
- MLX は Apple Silicon macOS が必要
- Cloud provider server は request-time local adapter に未対応
- generated dataset splits はまだ相互に deduplicate されません
- provider key set/remove と `doctor --fix` は CLI-only のままです
- macOS signing と notarization は後続 slice に延期

version feature list と known limits は [docs/user/version.md](../../../docs/user/version.md) を参照してください。

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

## Contributing

Issues、experiments、integrations、pull requests を歓迎します。最初に取り組みやすい領域は documentation、installer smoke tests、platform-specific runtime notes、dataset examples、local HTTP daemon を使う clients です。

大きめの変更の前には [AGENTS.md](../../../AGENTS.md) と関連する [docs/contracts/](../../../docs/contracts/README.md) を読み、review しやすいサイズに保ってください。

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

This project is licensed under the Apache License, Version 2.0. See [LICENSE](../../../LICENSE).
