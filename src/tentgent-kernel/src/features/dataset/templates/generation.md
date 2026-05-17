# Tentgent Dataset Generation Template

Template version: `{{template_version}}`
Canonical schema: `{{canonical_schema}}`
Task/domain hint: `{{task}}`
Language/content hint: `{{language}}`

You are generating tuning data for Tentgent.

Return only JSONL. Do not wrap the output in Markdown fences. Each line must be one complete JSON object.

Required output rules:

- Use `schema: "{{canonical_schema}}"` on every record.
- Use `messages` as the only conversation body.
- Supported message roles are `system`, `user`, `assistant`, and `tool`.
- For `train.jsonl`, `valid.jsonl`, and `test.jsonl`, each record must end with a final `assistant` answer inside `messages`.
- Use `tools` only to describe tools available to that record.
- Use assistant `tool_calls` for tool requests.
- Use `tool` messages for tool results.
- Keep `metadata` factual and non-training-critical.
- Do not use top-level `completion`, `answer`, `prompt`, `input`, or `output` fields.
- Do not output MLX, PEFT, ChatML, OpenAI-specific, or Anthropic-specific rendered prompt text.
- Keep generated content in language/content style `{{language}}` unless the task requires quoting another language.
- Prefer realistic, diverse, non-duplicated examples for task/domain `{{task}}`.

Training split JSONL examples for `train.jsonl`, `valid.jsonl`, and `test.jsonl`:

{"schema":"{{canonical_schema}}","id":"example-001","messages":[{"role":"system","content":"You are a concise assistant."},{"role":"user","content":"Say hello in one short sentence."},{"role":"assistant","content":"Hello! How can I help?"}],"metadata":{"task":"{{task}}","language":"{{language}}","source":"synthetic"}}
{"schema":"{{canonical_schema}}","id":"tool-example-001","messages":[{"role":"user","content":"Look up order A123."},{"role":"assistant","content":"","tool_calls":[{"id":"call_1","name":"lookup_order","arguments":{"order_id":"A123"}}]},{"role":"tool","tool_call_id":"call_1","name":"lookup_order","content":{"status":"shipped"}},{"role":"assistant","content":"Order A123 has shipped."}],"tools":[{"name":"lookup_order","description":"Look up one order status.","parameters":{"type":"object","properties":{"order_id":{"type":"string"}},"required":["order_id"]}}],"metadata":{"task":"tool_use","language":"{{language}}","source":"synthetic"}}

Eval case JSONL example for `eval_cases.jsonl`:

{"schema":"{{canonical_schema}}","id":"eval-example-001","messages":[{"role":"user","content":"Explain the refund policy briefly."}],"expected_behavior":{"answer_language":"{{language}}","checks":["answers directly","does not invent policy details"]},"metadata":{"task":"{{task}}","language":"{{language}}","source":"synthetic","split":"eval_cases"}}

When generating many rows:

- Use stable, unique `id` values.
- Keep one conversation per line.
- Avoid blank lines.
- Avoid trailing commas.
- Make sure every line parses as JSON independently.
- Before finalizing, internally check that the output would pass `tentgent dataset validate`.
