# Tentgent

Tentgent は Rust を主体としたローカル operator CLI で、Python daemon レイヤーを使って model runtime、adapter、LoRA training、長時間動作するローカル server、ローカル HTTP control plane を管理します。

現在の MVP は provider key の管理、ローカル model の取得と重複排除、LoRA adapter の import / pull、dataset 管理、単発 chat、LoRA adapter training、ローカル HTTP chat、主要なローカル workflow の daemon API に対応しています。

## 言語

- 英語 source of truth: [README.md](../../../README.md)
- 繁體中文: [docs/i18n/zh-TW/README.md](../zh-TW/README.md)
- 日本語: [docs/i18n/ja/README.md](./README.md)

## ツールをインストール

macOS ユーザー向けの推奨インストールは project Homebrew tap です:

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent runtime bootstrap
tentgent doctor
tentgent --version
```

Windows PowerShell ユーザー向けの推奨インストール:

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/latest/download/install.ps1 | iex
```

Linux x86_64 preview install は検証済み prerelease を使います:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.4-alpha.2/install.sh | bash
tentgent doctor
```

Linux preview は GitHub Release tarball とデフォルトの `base` runtime
bootstrap profile を使います。現時点では明示的な prerelease URL を使って
ください。stable `latest` release はまだ Linux support を advertise していません。
runtime data を default direct-installer support directory の外に置きたい場合は、
bootstrap 前に `TENTGENT_HOME` を設定して永続化してください。

再現可能な script-based setup にしたい場合は GitHub Release installer を使います:

```bash
curl -fsSL https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.sh | sh
```

```powershell
irm https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/install.ps1 | iex
```

以前 `install.sh` で install していた場合、`~/.local/bin/tentgent` が
Homebrew binary を `PATH` 上で shadow することがあります。Homebrew build
は直接確認できます:

```bash
/opt/homebrew/opt/tentgent/bin/tentgent -V
```

Homebrew install の upgrade は `brew upgrade hiroliang/tap/tentgent` を使います。`TENTGENT_HOME` 配下の user runtime data は保持されます。

install、upgrade、pinned version、local package smoke test、uninstall notes は [docs/user/install.md](../../../docs/user/install.md) を参照してください。

## Key を設定

ローカル runtime と provider key state を確認:

```bash
tentgent doctor
tentgent status
tentgent auth status
```

provider key を system Keychain に保存:

```bash
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

provider secret resolution と Keychain boundaries は [docs/contracts/auth-secrets.md](../../../docs/contracts/auth-secrets.md) を参照してください。

## Model を import / pull / remove

managed model を管理:

```bash
tentgent model pull hf-internal-testing/tiny-random-gpt2 --revision main
tentgent model ls
tentgent model inspect <model-ref>
tentgent model add /absolute/path/to/model
tentgent model rm <model-ref>
```

model、adapter、dataset、chat の完整な examples は [docs/user/commands.md](../../../docs/user/commands.md#models-and-chat) を参照してください。

## 単発 Chat

server を起動せず、1 回だけ local request を実行:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

単発 chat の message format と adapter examples は [docs/user/commands.md](../../../docs/user/commands.md#models-and-chat) を参照してください。

## Server を起動・停止して Chat

model-bound local server を起動:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
curl -sS http://127.0.0.1:8780/healthz
```

cloud provider server を起動:

```bash
tentgent server run openai:gpt-4.1-mini --host 127.0.0.1 --port 8780
tentgent server run claude:claude-sonnet-4-20250514 --host 127.0.0.1 --port 8781
```

server に直接 chat:

```bash
curl -sS http://127.0.0.1:8780/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

detached servers を管理:

```bash
tentgent server ls
tentgent server ps
tentgent server stop <server-ref>
```

Direct model-server chat は stateless です。session-aware chat には daemon を使ってください。server chat request と adapter rules は [docs/contracts/server-chat.md](../../../docs/contracts/server-chat.md) を参照してください。

## Daemon を起動・停止

```bash
tentgent daemon start --host 127.0.0.1 --port 8790
tentgent daemon status
curl -sS http://127.0.0.1:8790/healthz
curl -sS http://127.0.0.1:8790/v1/status
```

session-aware routing が必要な場合は、daemon から selected server に proxy します:

```bash
curl -sS http://127.0.0.1:8790/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"server_ref":"<server-ref>","messages":[{"role":"user","content":"Hello"}],"stream":false}'
```

daemon を停止:

```bash
tentgent daemon stop
```

完整な daemon API、endpoints、response shapes、auth、error mapping は [docs/contracts/http-daemon.md](../../../docs/contracts/http-daemon.md) を参照してください。

## TUI に入る

```bash
tentgent tui
```

TUI は daemon discovery、chat、jobs、resources、store、server、training、guarded setup flows のための operator console です。

## ツールを削除

Homebrew uninstall は installed binary と support files だけを削除し、
user runtime data は残します:

```bash
brew uninstall hiroliang/tap/tentgent
```

直接 `install.sh` で install した場合は、これらの files を削除します:

```bash
rm -f "$HOME/.local/bin/tentgent"
rm -rf "$HOME/.local/share/tentgent"
```

Linux preview install では、`$HOME/.local/share/tentgent` が default
runtime home でもある場合があります。runtime data を消したい場合、または
`TENTGENT_HOME` で runtime data を別の場所に置いた場合以外は、この
directory を削除しないでください。

safe-to-recreate bootstrap cache は必要に応じて削除できます:

```bash
rm -rf "$TENTGENT_HOME/runtime/bootstrap/uv-cache"
```

models、adapters、datasets、sessions、servers、train records、その他 local runtime data を消したい場合以外、`TENTGENT_HOME` は削除しないでください。uninstall と runtime-home の詳細は [docs/user/install.md](../../../docs/user/install.md) と [docs/user/runtime.md](../../../docs/user/runtime.md) を参照してください。

## Version Notes

`v0.3.4-alpha.2` は Linux x86_64 preview release です。release tarball
install、default base runtime bootstrap、Ubuntu 24.04 Docker smoke 済みの
`doctor` readiness を含みます。

`v0.3.3` は Homebrew tap update tooling を追加し、stable release 後の formula URL と checksum 更新を repeatable にします。

`v0.3.2` は `tentgent runtime bootstrap` を追加し、Homebrew / package-manager install 後の managed Python runtime setup を公開 CLI 入口にします。

`v0.3.1` は 0.3.x stable hotfix です。macOS release binary の ad-hoc signing と installer 後の quarantine cleanup を追加し、ダウンロード後に macOS が binary を直接 kill するケースを減らします。

version feature list と known limits は [docs/user/version.md](../../../docs/user/version.md) を参照してください。

## 完整な CLI コマンド

README は最短ルートだけを載せます。完整な CLI command reference は [docs/user/commands.md](../../../docs/user/commands.md) を参照してください。TUI、auth、models、adapters、datasets、chat、servers、daemon、sessions、LoRA training を含みます。

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

## 現在の機能

- Hugging Face、OpenAI、Anthropic の provider auth key 管理
- content-addressed model、adapter、dataset stores
- OpenAI と Anthropic の local server proxy runtimes
- dataset validation、prompt templates、multi-split provider synthesis、provider evaluation
- MLX、PEFT safetensors、llama-cpp GGUF path の one-shot local chat
- store、dataset、server、chat、training、diagnostics、bounded session workflow を扱う local HTTP daemon API
- daemon discovery、chat、jobs、resources、store/server/training actions、session cleanup、guarded local setup のための terminal UI operator console
- managed LoRA train plans、durable run records、metrics/log inspection、実行可能な MLX / PEFT training loops
- bounded transcript compaction を備えた local sessions
- 通常 installer install 用の Python runtime bootstrap と、package-manager install 用の `tentgent runtime bootstrap`

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
