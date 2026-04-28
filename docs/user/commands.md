# Common Commands

This document collects user-facing command examples. Short references are accepted anywhere a local `model_ref`, `adapter_ref`, `dataset_ref`, or `server_ref` is requested, as long as the prefix is unique.

## Auth

Check all provider keys:

```bash
tentgent auth status
```

Set provider keys:

```bash
tentgent auth hf set
tentgent auth openai set
tentgent auth anthropic set
```

Inspect or remove one provider key:

```bash
tentgent auth openai
tentgent auth openai rm
```

## Models

Pull models from Hugging Face:

```bash
tentgent model pull google/gemma-3-1b-it
tentgent model pull mlx-community/Llama-3.2-1B-Instruct-4bit
tentgent model pull DravenBlack/gemma-3-1b-it-Q4_K_M-GGUF
```

List and inspect models:

```bash
tentgent model ls
tentgent model inspect <model-ref>
```

## Chat

Run one-shot chat:

```bash
tentgent chat <model-ref> --message "user:Hello there"
```

Run one-shot chat with an adapter:

```bash
tentgent chat <model-ref> \
  --adapter-ref <adapter-ref> \
  --message "user:Think step by step: what is 12 * 7?" \
  --max-tokens 128
```

## Server

Launch a long-lived local server:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load
```

Call the server:

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

Use background server mode:

```bash
tentgent server run <model-ref> --host 127.0.0.1 --port 8780 --lazy-load --detach
tentgent server ls
tentgent server ps
tentgent server stop <server-ref>
```

Run a cloud provider server:

```bash
tentgent auth openai set
tentgent server run openai:gpt-4.1-mini --host 127.0.0.1 --port 8780
tentgent server ls
```

Cloud provider servers run as local HTTP proxies. Provider keys are resolved at launch and are not written to `server.toml`.

## Adapters

Import or pull adapters:

```bash
tentgent adapter add /path/to/adapter --base-model-ref <model-ref>
tentgent adapter pull <hf-adapter-repo> --base-model-ref <model-ref>
tentgent adapter ls
```

Adapter requests should visibly change answer style when the adapter is compatible with the base model.

## Datasets

Import local datasets for training or evaluation:

```bash
tentgent dataset add /path/to/dataset.jsonl
tentgent dataset add /path/to/dataset-dir
tentgent dataset ls
tentgent dataset inspect <dataset-ref>
tentgent dataset export <dataset-ref> /path/to/work-dir
tentgent dataset diff <left-dataset-ref> <right-dataset-ref>
tentgent dataset diff <dataset-ref> --path /path/to/work-dir
tentgent dataset rm <dataset-ref>
```

A training dataset directory is ready when it contains `train.jsonl`. Optional companions include `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json`.

New chat and tool-use datasets should use the canonical `tentgent.chat.v1` schema in [docs/contracts/dataset-schema.md](../contracts/dataset-schema.md).

To edit a managed dataset, export it to a working directory, edit there, then run `dataset add` again to create a new content-derived reference.

## LoRA Training

Create, inspect, and run a managed LoRA training plan:

```bash
tentgent train lora plan create \
  --model <model-ref> \
  --dataset <dataset-ref> \
  --interactive
tentgent train lora plan ls
tentgent train lora plan inspect <plan-ref>
tentgent train lora plan rm <plan-ref>
tentgent train lora run <plan-ref>
```

Tentgent auto-selects the backend from the model format: `mlx` models use MLX, `safetensors` models use PEFT, and `gguf` models are blocked for LoRA training.

Common plan overrides: `--rank`, `--learning-rate`, `--batch-size`, `--grad-accum`, `--max-steps`, `--seed`, and `--max-seq-length`.

Use `--mask-prompt` for chat-style datasets when you want the model to see system, user, and tool context but train loss only on assistant output.
