# Tentgent 中文入口

Tentgent 是管理本地模型、推論、資料集、LoRA 訓練與 HTTP 服務的 CLI 工具。依下列目的進入功能頁，即可找到操作範例、參數與 API 格式。詳細文件以英文為準。

## 快速開始

```bash
brew tap hiroliang/tap
brew install hiroliang/tap/tentgent
tentgent --version
tentgent runtime bootstrap --profile local-model
tentgent doctor
tentgent model pull HuggingFaceTB/SmolLM-135M-Instruct
tentgent chat <model-ref> --message "user:Hello" --max-tokens 64
```

以 pull/list 輸出的 ref 取代 `<model-ref>`。Windows、Linux 與版本選擇請看[安裝文件](../../user/install.md)。

<a id="文件入口"></a>

## 依目的找功能

| 目的 | 指令 | 功能說明 |
| --- | --- | --- |
| 安裝、升級與移除 | `--version` | [安裝](../../user/install.md) |
| 準備 runtime、查看後端 | `runtime bootstrap/status` | [Runtime](../../user/runtime.md) |
| 設定金鑰、切換來源 | `auth` | [認證](../../user/auth.md) |
| 診斷、修復與清理 | `doctor / runtime reconcile / store gc` | [維護](../../user/maintenance.md) |
| 模型、能力與支援證據 | `model` | [模型](../../user/models.md) |
| 匯入、綁定、移除 adapter | `adapter` | [Adapters](../../user/adapters.md) |
| 產生、驗證、匯入與評估資料 | `dataset` | [Datasets](../../user/datasets.md) |
| 建立 plan、訓練並使用 LoRA | `train lora` | [LoRA 訓練](../../user/training-lora.md) |
| 文字對話與串流 | `chat` | [Chat](../../user/inference/chat.md) |
| 文字向量與文件排序 | `embed / rerank` | [Embedding / rerank](../../user/inference/embedding-rerank.md) |
| 語音轉文字、合成語音 | `transcribe / speak` | [音訊](../../user/inference/audio.md) |
| 詢問圖片內容 | `vision chat` | [Vision](../../user/inference/vision.md) |
| 理解影片 | `video understand` | [Video](../../user/inference/video.md) |
| 產生圖片 | `image generate` | [圖片生成](../../user/inference/images/generate.md) |
| 改圖、遮罩重繪、ControlNet | `image transform/inpaint/control` | [圖片編輯](../../user/inference/images/edit.md) |
| 建立管理用 HTTP 入口 | `daemon` | [Daemon、host 與認證](../../user/daemon.md) |
| 建立本地或雲端模型服務 | `server` | [Servers](../../user/servers.md) |
| 多模型共用一個服務入口 | `cluster` | [Clusters](../../user/clusters.md) |
| 保存有界對話內容 | `session / chat --session` | [Sessions](../../user/sessions.md) |
| 查詢、取消與清理工作 | `/v1/jobs` | [Jobs](../../user/jobs.md) |

## 其他文件

- [English README](../../../README.md) · [User guide](../../user/README.md)
- [Command index](../../user/commands.md) · [HTTP API index](../../user/api.md)
- [OpenAI](../../user/providers/openai.md) · [Anthropic / Claude](../../user/providers/anthropic.md) · [Gemini](../../user/providers/gemini.md)
- [Provider compatibility](../../user/provider-compatibility.md) · [Base URLs](../../user/providers/README.md#base-urls)
- [Model fixtures](../../user/model-fixtures.md) · [Support catalog](../../user/model-support-catalog.md)
- [Version notes](../../user/version.md) · [1.0 readiness](../../user/1.0-readiness.md)
- [Developer guide](../../development/README.md) · [Contracts](../../contracts/README.md)

## 語言

[English](../../../README.md) · [繁體中文](../zh-TW/README.md) · [日本語](../ja/README.md) · [Languages](../README.md)
