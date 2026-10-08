# Tentgent 日本語入口

Tentgent はローカルモデル、推論、データセット、LoRA 学習、HTTP サービスを管理する CLI ツールです。目的から機能ガイドを開くと、操作例、パラメーター、API 形式を確認できます。詳細は英語版を正とします。

## クイックスタート

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent --version
tentgent runtime bootstrap --profile local-model
tentgent doctor
tentgent model pull HuggingFaceTB/SmolLM-135M-Instruct
tentgent chat <model-ref> --message "user:Hello" --max-tokens 64
```

`<model-ref>` は pull/list が返す ref に置き換えてください。Windows、Linux、バージョン指定は[インストールガイド](../../user/install.md)を参照してください。

<a id="docs"></a>

<a id="quick-start"></a>

## 目的から機能を探す

| 目的 | コマンド | ガイド |
| --- | --- | --- |
| インストール、更新、削除 | `--version` | [インストール](../../user/install.md) |
| ランタイムとバックエンドの準備 | `runtime bootstrap/status` | [Runtime](../../user/runtime.md) |
| API キーと取得元の設定 | `auth` | [認証](../../user/auth.md) |
| 診断、修復、クリーンアップ | `doctor / runtime reconcile / store gc` | [メンテナンス](../../user/maintenance.md) |
| モデル、機能、対応状況の確認 | `model` | [モデル](../../user/models.md) |
| アダプターの取り込みと関連付け | `adapter` | [Adapters](../../user/adapters.md) |
| データの生成、検証、評価 | `dataset` | [Datasets](../../user/datasets.md) |
| 学習計画の作成と LoRA の利用 | `train lora` | [LoRA 学習](../../user/training-lora.md) |
| テキスト対話とストリーミング | `chat` | [Chat](../../user/inference/chat.md) |
| ベクトル化と文書の順位付け | `embed / rerank` | [Embedding / rerank](../../user/inference/embedding-rerank.md) |
| 文字起こしと音声合成 | `transcribe / speak` | [音声](../../user/inference/audio.md) |
| 画像について質問 | `vision chat` | [Vision](../../user/inference/vision.md) |
| 動画の理解 | `video understand` | [Video](../../user/inference/video.md) |
| 画像の生成 | `image generate` | [画像生成](../../user/inference/images/generate.md) |
| 画像変換、修復、ControlNet | `image transform/inpaint/control` | [画像編集](../../user/inference/images/edit.md) |
| 管理用 HTTP API の起動 | `daemon` | [Daemon、host、認証](../../user/daemon.md) |
| ローカル・クラウドモデルの提供 | `server` | [Servers](../../user/servers.md) |
| 複数モデルへのルーティング | `cluster` | [Clusters](../../user/clusters.md) |
| 会話コンテキストの保存 | `session / chat --session` | [Sessions](../../user/sessions.md) |
| ジョブの確認、キャンセル、削除 | `/v1/jobs` | [Jobs](../../user/jobs.md) |

## 関連ドキュメント

- [English README](../../../README.md) · [User guide](../../user/README.md)
- [Command index](../../user/commands.md) · [HTTP API index](../../user/api.md)
- [OpenAI](../../user/providers/openai.md) · [Anthropic / Claude](../../user/providers/anthropic.md) · [Gemini](../../user/providers/gemini.md)
- [Provider compatibility](../../user/provider-compatibility.md) · [Base URLs](../../user/providers/README.md#base-urls)
- [Model fixtures](../../user/model-fixtures.md) · [Support catalog](../../user/model-support-catalog.md)
- [Version notes](../../user/version.md) · [1.0 readiness](../../user/1.0-readiness.md)
- [Developer guide](../../development/README.md) · [Contracts](../../contracts/README.md)

## 言語

[English](../../../README.md) · [繁體中文](../zh-TW/README.md) · [日本語](../ja/README.md) · [Languages](../README.md)
