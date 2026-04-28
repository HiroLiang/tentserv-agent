# Cloud Dataset MVP

Use OpenAI and Claude to help users produce valid Tentgent tuning data. This track should reuse the provider client boundary from [cloud-provider-server-mvp.md](./cloud-provider-server-mvp.md) once cloud chat routing is stable.

## Scope

- Reuse `auth openai` and `auth anthropic` secret resolution.
- Generate, validate, and evaluate `tentgent.chat.v1` JSONL packages.
- Support both provider-backed generation and manual AI/agent generation through a fixed prompt template.
- Keep generated data file-first until the user explicitly imports it with `dataset add`.

## Non-Goals

- Do not add cloud providers to the model store.
- Do not automatically train after generation.
- Do not auto-publish generated datasets or adapters.

## Command Surface

Implemented foundation commands:

```text
tentgent dataset validate <PATH>
tentgent dataset template [--task <KIND>] [--language <LANG>] [--output <PATH>]
```

Planned provider-backed commands:

```text
tentgent dataset synth --provider <openai|anthropic> --output <DIR> (--brief <TEXT> | --spec <PATH>) [OPTIONS]
tentgent dataset eval <DATASET_REF|PATH> --provider <openai|anthropic> [OPTIONS]
```

Command intent:

- `validate`
  Check local files before import or training.
- `template`
  Print a stable Markdown prompt users can paste into OpenAI, Claude, or another agent to create compliant JSONL.
- `synth`
  Call OpenAI or Claude directly and write a local dataset package.
- `eval`
  Ask a provider to review a generated or managed dataset and write a local report.

## Execution Order

### Slice 1: Dataset Validate

Implement `dataset validate <PATH>` first.

Status: implemented.

Goals:

- validate single JSONL files and dataset directories
- validate `train.jsonl`, `valid.jsonl`, `test.jsonl`, and `eval_cases.jsonl` when present
- report split counts and tuning readiness
- surface schema errors with file path and line number
- avoid network calls

Review target:

- users can check manually generated data before `dataset add`

### Slice 2: Manual Generation Template

Implement `dataset template`.

Status: implemented.

Goals:

- generate one paste-ready Markdown prompt for OpenAI, Claude, or another agent
- include the required `tentgent.chat.v1` output rules
- include one minimal JSONL example
- support task/language hints without expanding into a complex prompt builder
- keep the output deterministic enough that validation failures are actionable

Review target:

- users can create valid tuning data even when they do not know the dataset contract

### Slice 3: Provider Client Reuse

Reuse the shared provider boundary from the cloud provider server track.

Goals:

- keep dataset prompts and parsing outside provider transport code
- support explicit provider model selection for generation and evaluation
- fail clearly when keys, network, or provider output are unavailable
- avoid duplicating OpenAI or Anthropic request/response code

Review target:

- dataset generation can call providers without owning provider transport details

### Slice 4: Dataset Synth

Implement a file-first `dataset synth` draft.

Goals:

- accept either `--brief` or `--spec`
- write `train.jsonl` by default
- optionally write `valid.jsonl`, `test.jsonl`, and `eval_cases.jsonl`
- write a source `manifest.json` with provider, provider model, prompt template version, and generation options
- run local validation before printing success
- print the suggested `tentgent dataset add <DIR>` command

Review target:

- generated output is immediately inspectable and importable, but not automatically managed

### Slice 5: Dataset Eval

Implement `dataset eval` for local paths and managed datasets.

Goals:

- evaluate whether answers stay inside provided context
- flag language mismatch, hallucination risk, unsafe requests, malformed tool calls, and format drift
- write a local report without mutating the dataset
- keep report paths predictable under the chosen output directory

Review target:

- users get a review artifact before training on generated data

## Dataset Contract

All generated records must use [../contracts/dataset-schema.md](../contracts/dataset-schema.md).

Providers must output JSONL where each row has:

- `schema = "tentgent.chat.v1"`
- `messages`
- optional `tools`
- optional `metadata`

Generated data must not be pre-rendered as MLX, PEFT, ChatML, or provider-specific prompt text.

## Default Decisions

- Accept both `--brief` and `--spec`; require exactly one.
- Default `synth` output is `train.jsonl`; add split flags later only when needed.
- Store provider/model/template metadata in source `manifest.json` before `dataset add`.
- Keep validation local and deterministic; providers are used only by `synth` and `eval`.
