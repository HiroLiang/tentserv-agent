# Dataset Schema

This document defines Tentgent's canonical dataset schema for chat tuning, tool-use tuning, behavior evaluation, and future cloud-generated datasets.

The current canonical schema id is:

```text
tentgent.chat.v1
```

## Goals

- Give users and agents one stable format to write.
- Let OpenAI, Claude, or local models generate datasets without knowing MLX or PEFT internals.
- Let Tentgent render the same dataset into MLX, PEFT, evaluation, and future server-test formats.
- Preserve structured tool calls instead of forcing users to hand-render backend-specific text.

## Package Layout

Training packages should use JSONL split files:

```text
<dataset-dir>/
├── train.jsonl
├── valid.jsonl
├── test.jsonl
├── eval_cases.jsonl
└── manifest.json
```

Only `train.jsonl` is required for tuning. `valid.jsonl`, `test.jsonl`, `eval_cases.jsonl`, and source `manifest.json` are optional.

Each JSONL line is one independent record.

## Record Shape

```json
{
  "schema": "tentgent.chat.v1",
  "id": "optional-stable-id",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Use the available tool to fetch Hiro's role."},
    {
      "role": "assistant",
      "content": "",
      "tool_calls": [
        {
          "id": "call_1",
          "name": "get_profile",
          "arguments": {"field": "current_role"}
        }
      ]
    },
    {
      "role": "tool",
      "tool_call_id": "call_1",
      "name": "get_profile",
      "content": {"current_role": "AI Engineer"}
    },
    {"role": "assistant", "content": "Hiro Liang is currently an AI Engineer."}
  ],
  "tools": [
    {
      "name": "get_profile",
      "description": "Fetch one public profile field.",
      "parameters": {
        "type": "object",
        "properties": {"field": {"type": "string"}},
        "required": ["field"]
      }
    }
  ],
  "metadata": {
    "task": "tool_use",
    "language": "en",
    "source": "synthetic"
  }
}
```

## Required Fields

- `messages`: non-empty ordered conversation messages.

Optional fields:

- `schema`: defaults to `tentgent.chat.v1` when absent during import or rendering.
- `id`: user-provided stable record id.
- `tools`: tool definitions available to this record.
- `metadata`: non-training metadata unless a renderer explicitly opts in.

## Message Roles

Supported roles:

- `system`: global instruction or behavior policy.
- `user`: user input.
- `assistant`: assistant text or assistant tool-call request.
- `tool`: tool result paired to a prior assistant tool call.

Role rules:

- `system` and `user` messages require non-empty string `content`.
- `assistant.content` may be empty only when `tool_calls` is non-empty.
- `tool` messages require `tool_call_id`, `name`, and `content`.
- A record used with prompt masking should end with a final assistant answer.

## Content Rules

- Text messages should use string `content`.
- Tool result `content` may be a string, object, array, number, boolean, or null.
- Non-string tool content is canonicalized to deterministic JSON before rendering.
- Empty strings are invalid except for assistant tool-call messages.

## Tool Call Shape

Canonical assistant tool calls use:

```json
{
  "id": "call_1",
  "name": "get_profile",
  "arguments": {"field": "current_role"}
}
```

Rules:

- `id` is required and must be unique inside one record.
- `name` is required.
- `arguments` is required and should be a JSON object.
- OpenAI-style `{ "type": "function", "function": { "name": "...", "arguments": "{\"x\":1}" } }` may be accepted as input, but Tentgent should canonicalize it into the shape above.

## Renderer Contract

Users write `tentgent.chat.v1`. Backend renderers convert it.

The renderer is shared and backend-neutral. PEFT and MLX must consume the same canonical record through the same Tentgent rendering rules before backend-specific tokenization or file writing happens.

Renderer responsibilities:

- Preserve message order.
- Preserve tool-call id, tool name, and arguments.
- Use tokenizer chat templates when they support the needed schema.
- Fall back to Tentgent's stable text rendering when tokenizer templates cannot render tools.
- Apply prompt masking after rendering, not before.

Fallback text rendering should be stable across MLX and PEFT. A tool call should render as structured text equivalent to:

```text
Assistant tool_call call_1 get_profile {"field":"current_role"}
Tool result call_1 get_profile {"current_role":"AI Engineer"}
```

The exact token wrapper can evolve, but MLX and PEFT must share the same Tentgent renderer for the same dataset record.

Backend contract:

- PEFT consumes rendered text and then builds tokenizer labels.
- MLX consumes rendered text by writing or exposing MLX-compatible JSONL/text records.
- Training splits render to `text` when prompt masking is disabled.
- Training splits render to `prompt` plus `completion` when prompt masking is enabled.
- `eval_cases.jsonl` is validated as canonical records but is not direct trainer input.
- Both backends must preserve tool calls and tool results using the same fallback text rendering.
- Backend-specific tokenizer chat templates may be used only when they can represent the same semantic structure.

## Cloud Generation Contract

Future `dataset synth` providers must output `tentgent.chat.v1` JSONL.

Prompt requirements for OpenAI or Claude dataset generation:

- Return JSONL, one complete record per line.
- Use `schema = "tentgent.chat.v1"` on every generated record.
- Use `messages` as the only training conversation body.
- Use `tools` only to describe tools available to that record.
- Use assistant `tool_calls` for tool requests.
- Use `tool` messages for tool results.
- Keep `metadata` factual and non-training-critical.
- Do not pre-render MLX, PEFT, ChatML, or provider-specific text formats.

## Compatibility

- MLX and PEFT must both consume `tentgent.chat.v1` through Tentgent renderers.
- GGUF is an inference artifact in this project and is not a LoRA training target.
- Legacy `prompt` plus `completion` and plain `text` records may remain accepted for simple datasets, but new generated datasets should use `messages`.
