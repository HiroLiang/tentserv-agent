# Cloud Dataset MVP

Status: archived. This plan completed dataset validation, deterministic generation templates, provider-backed dataset synthesis, synthesis debugging, and provider-backed dataset evaluation.

Use OpenAI and Claude to help users produce valid Tentgent tuning data. This track reused the provider client boundary from [cloud-provider-server-mvp.md](../cloud-provider-server-mvp.md) once cloud chat routing became stable.

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

Implemented commands:

```text
tentgent dataset validate <PATH>
tentgent dataset template [-t|--task <KIND>] [-l|--language <LANG>] [-o|--output <PATH>]
tentgent dataset synth -p|--provider <openai|anthropic|claude> -m|--model <MODEL> -o|--output <DIR> (-b|--brief <TEXT> | -s|--spec <PATH>) [OPTIONS]
tentgent dataset synth -P|--print-prompt (-b|--brief <TEXT> | -s|--spec <PATH>) [OPTIONS]
tentgent dataset eval <DATASET_REF|PATH> -p|--provider <openai|anthropic|claude> -m|--model <MODEL> -o|--output <DIR> [OPTIONS]
```

Command intent:

- `validate`
  Check local files before import or training.
- `template`
  Print a stable Markdown prompt users can paste into OpenAI, Claude, or another agent to create compliant JSONL.
- `synth`
  Call OpenAI or Claude directly and write a local dataset package, or print the exact provider prompt with `--print-prompt`.
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

Status: implemented.

Goals:

- keep dataset prompts and parsing outside provider transport code
- support explicit provider model selection for generation and evaluation
- fail clearly when keys, network, or provider output are unavailable
- avoid duplicating OpenAI or Anthropic request/response code

Review target:

- dataset generation can call providers without owning provider transport details

Implementation notes:

- `tentgent_daemon.datasets.provider.call_dataset_provider` reuses the shared provider chat client for OpenAI and Anthropic calls.
- `generate_dataset_jsonl` builds dataset-specific provider messages, parses JSONL from raw provider text, and validates generated records through the same Python renderer used by MLX and PEFT preparation.
- Provider JSONL parsing accepts pure JSONL, fenced JSONL, and JSONL wrapped in provider prose; malformed or backend-incompatible rows fail before file writing.

### Slice 4: Dataset Synth

Implement a file-first `dataset synth` draft.

Status: implemented.

Goals:

- accept either `--brief` or `--spec`
- write `train.jsonl` by default
- optionally write `valid.jsonl`, `test.jsonl`, and `eval_cases.jsonl`
- write a source `manifest.json` with provider, provider model, prompt template version, and generation options
- run local validation before printing success
- print the suggested `tentgent dataset add <DIR>` command

Review target:

- generated output is immediately inspectable and importable, but not automatically managed

Implementation notes:

- Rust CLI performs provider auth preflight and passes the selected key to the Python runtime through the provider environment variable.
- Python writes only to a missing or empty output directory and emits a JSON summary for the Rust CLI to render.
- The generated package contains the requested split JSONL plus `manifest.json`; the user must run `dataset validate` and `dataset add` explicitly.

### Slice 4.1: Dataset Synth Debug And CLI Short Flags

Harden `dataset synth` and improve command help after real provider testing.

Status: implemented.

Goals:

- add `--print-prompt` / `-P` to show the exact provider prompt without auth or network calls
- write provider parse-failure debug artifacts under an empty output directory
- add conservative short aliases for common CLI options across command groups
- keep `-h, --help` visible and ensure help text describes new aliases

Review target:

- users can copy/debug provider prompts and discover common short flags directly from `--help`

Implementation notes:

- Failed provider parsing writes `_debug/prompt.md`, `_debug/provider-output.raw.txt`, and `_debug/error.txt` when the requested output directory is missing or empty.
- Common short flags now include examples such as `dataset synth -p/-m/-o/-b/-s`, `dataset template -t/-l/-o`, `server run -H/-a/-p/-d`, and `chat -m/-n/-T/-s`.

### Slice 5: Dataset Eval

Implement `dataset eval` for local paths and managed datasets.

Status: implemented.

Goals:

- evaluate whether answers stay inside provided context
- flag language mismatch, hallucination risk, unsafe requests, malformed tool calls, and format drift
- write a local report without mutating the dataset
- keep report paths predictable under the chosen output directory

Review target:

- users get a review artifact before training on generated data

Implementation notes:

- Rust accepts either a local dataset path or a managed dataset reference and passes the resolved source path to the Python runtime.
- Python samples records from `train`, `valid`, `test`, `eval_cases`, or all detected splits, then asks the selected provider for a structured `tentgent.dataset.eval.report.v1` report.
- Reports are file-first and non-mutating: `eval-report.json`, `eval-report.md`, `prompt.md`, and `provider-output.raw.txt` are written under the requested output directory.
- `--criteria` / `-c` supports project-specific checks such as language quality, style drift, refusal behavior, or a desired verbal habit.

## Dataset Contract

All generated records must use [dataset-schema.md](../../contracts/dataset-schema.md).

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
