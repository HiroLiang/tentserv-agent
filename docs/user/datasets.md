# Datasets

Use `tentgent dataset` to generate, validate, import, inspect, export, compare,
and evaluate training data. For the record format and split filenames, see
[dataset-schema.md](../contracts/dataset-schema.md). For managed identity and
storage, see [dataset-store.md](../contracts/dataset-store.md).

## Local Dataset Commands

```bash
tentgent dataset template --task chat --language en --output dataset-template.md
tentgent dataset validate /path/to/dataset-dir
tentgent dataset add /path/to/dataset-dir
tentgent dataset ls
tentgent dataset inspect <dataset-ref>
tentgent dataset export <dataset-ref> /path/to/working-copy
tentgent dataset diff <left-ref> <right-ref>
tentgent dataset diff <dataset-ref> --path /path/to/working-copy
tentgent dataset rm <dataset-ref>
```

`template` writes a generation prompt, without calling a provider. `validate`
checks local data before import. `add` imports a snapshot and prints its managed
ref. Export, edit, and import again to create a new content-derived ref. Export
requires a missing or empty destination directory. Removing a managed dataset
does not remove exported copies.

## Local Command Parameters

| Command | Inputs and options |
| --- | --- |
| `add`, `validate` | Local file or directory path. Validation does not import. |
| `template` | `--task` and `--language` select prompt intent; `--output` writes a file instead of stdout. |
| `ls`, `inspect`, `rm` | List all records, inspect one ref, or remove one managed ref. |
| `export` | Managed ref and positional destination path; the destination must be missing or empty. |
| `diff` | Left managed ref plus either a right managed ref or `--path` to a working copy. |

Run `tentgent dataset <command> --help` for the installed version's aliases.

## Synthesis Parameters

The CLI currently accepts `openai`, `anthropic`, and the `claude` alias.
Gemini is accepted by the daemon dataset API, but is rejected by the current
CLI argument parser. Provider auth follows the configured
[auth source mode](./auth.md).

```bash
tentgent dataset synth \
  --provider openai --model <provider-model> \
  --brief 'Generate short everyday questions with one-sentence helpful answers.' \
  --train-count 60 --valid-count 10 --test-count 10 \
  --max-tokens 8192 --output ./generated-dataset
tentgent dataset validate ./generated-dataset
tentgent dataset add ./generated-dataset
```

Synthesis writes local split files. Import is a separate step. Counts are sent
as instructions to the provider; inspect the generated records and run
`validate` before training.

| CLI option | Meaning |
| --- | --- |
| `-p`, `--provider` | Required for generation; provider choices are listed above. |
| `-m`, `--model` | Required provider model name, not a managed model ref. |
| `-o`, `--output` | Required local output directory for generated files. |
| `-b`, `--brief` | Inline generation request; choose exactly one of brief or spec. |
| `-s`, `--spec` | Local UTF-8 specification or edited template file. |
| `-S`, `--split` | Single split: `train` (default), `valid`, `test`, or `eval_cases`. |
| `--count` | Requested record count for a single split. |
| `--train-count`, `--valid-count`, `--test-count`, `--eval-count` | Requested counts for several split files in one invocation. |
| `-n`, `--max-tokens` | Provider output token budget per request; runtime default is 4096. |
| `-T`, `--temperature` | Sampling temperature; default `0`. |
| `--timeout-seconds` | Accepted request setting; default `180`. See execution limits below. |
| `-r`, `--retries` | Accepted synthesis retry setting; default `1`. See execution limits below. |
| `-P`, `--print-prompt` | Print the prompt without provider auth or network calls; provider, model, and output are unnecessary. |

Use either `--split` with `--count`, or split-specific counts. For a single
split, always supply `--count`; do not rely on an omitted count.

## Evaluation Parameters

```bash
tentgent dataset eval <dataset-ref-or-path> \
  --provider openai --model <provider-model> \
  --split all --max-records 20 --output ./dataset-report
```

Evaluation reviews dataset records through a cloud provider; it does not run
the local base model or trained adapter. The input dataset is unchanged.
The report directory must be missing or empty and separate from the dataset.

| CLI option | Meaning |
| --- | --- |
| `-p`, `--provider`; `-m`, `--model`; `-o`, `--output` | Required provider, provider model, and report directory. |
| `-S`, `--split` | `train` (default), `valid`, `test`, `eval_cases`, or `all`. |
| `-n`, `--max-records` | Maximum input records sent for review; default `20`. |
| `-c`, `--criteria` | Additional review criteria. |
| `--max-tokens` | Provider output budget; runtime default `4096`. Unlike synth, `-n` selects input record count. |
| `-T`, `--temperature`; `--timeout-seconds` | Accepted settings, default `0` and `180`. |

## HTTP Request Fields

The [API route table](#http-routes) covers deterministic dataset tools.
Cloud synthesis and evaluation use `POST /v1/datasets/synth/jobs` and
`POST /v1/datasets/eval/jobs`. Both return HTTP `202` with a `job` object;
inspect `GET /v1/jobs/{job_id}` for completion and artifact paths. Paths in
these JSON requests belong to the daemon host. Provider auth is resolved on
that host; [daemon bearer auth](./api.md) protects the HTTP request separately.

Shared required fields are `provider`, `model`, and `output_path` (strings).
API providers are `openai`, `anthropic`, and `gemini`; use `anthropic` for
Claude. `output_path` must be absolute and missing or empty.

Synthesis example:

```json
{
  "provider": "openai",
  "model": "<provider-model>",
  "output_path": "/absolute/path/generated-dataset",
  "brief": "Generate short everyday questions with one-sentence helpful answers.",
  "train_count": 60,
  "valid_count": 10,
  "test_count": 10,
  "max_tokens": 8192
}
```

| Synthesis field | Type and rule |
| --- | --- |
| `brief`, `spec_content`, `spec_path` | Choose exactly one string; `spec_path` is an absolute daemon-host path. |
| `split`, `count` | Single-split mode requires both a split string and positive integer count. |
| `train_count`, `valid_count`, `test_count`, `eval_count` | Alternative split-specific integer counts; at least one must be positive. Do not combine with `split` or `count`. |
| `max_tokens`, `temperature` | Optional positive integer output budget and numeric sampling temperature. |
| `timeout_seconds`, `retries` | Optional numeric timeout and integer retry setting. See execution limits below. |
| `print_prompt` | Omit or use `false`; `true` is rejected by this asynchronous endpoint. Use CLI `--print-prompt`. |

Evaluation example:

```json
{
  "provider": "openai",
  "model": "<provider-model>",
  "output_path": "/absolute/path/dataset-report",
  "dataset_ref": "<dataset-ref>",
  "split": "all",
  "max_records": 20,
  "criteria": "Check answer relevance and consistency."
}
```

| Evaluation field | Type and rule |
| --- | --- |
| `dataset_ref`, `input_content`, `input_path` | Choose exactly one string. `input_path` is an absolute daemon-host path. |
| `input_format` | Accepted only with `input_content`; currently `jsonl` only. |
| `split`, `max_records` | Optional split string (default `train`) and positive integer input limit (default `20`). |
| `criteria` | Optional string with review criteria. |
| `max_tokens`, `temperature`, `timeout_seconds` | Optional output budget, sampling temperature, and accepted timeout setting. |

### Validation And Managed Responses

```bash
curl -sS -X POST http://127.0.0.1:8790/v1/datasets/validate \
  -H "Authorization: Bearer $TENTGENT_DAEMON_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"path":"/absolute/path/on/daemon-host/dataset"}'
```

Validation returns `valid`, `tuning_ready`, `records`, `errors_count`, per-split
counts, warnings, and errors with path/line/message. Invalid dataset content
returns HTTP `200` with `valid: false`; malformed HTTP input returns `400`.
Supply exactly one of `path` and `dataset_ref`.

List returns `datasets`; inspect returns `dataset`; import adds `mutation`
metadata. Template returns its prompt as `content` and writes no file.
Export returns `dataset` and `export` metadata; it writes on the daemon host.
Diff returns `left`, `right`, and `diff`; it includes at most 500 file entries
and sets `diff.truncated` when more exist. For diff, select exactly one of
`right_dataset_ref` and `right_path`.

## Current Execution Limits

The Rust dataset client in this source checkout sends one generation request per selected split
and writes the returned text after removing Markdown fences. It does not
enforce the requested output record count or retry invalid provider output.
The accepted `retries` and `timeout_seconds` values are not wired into this
client's request execution. `dataset eval` writes `prompt.md` and `report.json`
with the provider's review text; a completed review does not establish model
quality. Use explicit validation and inspect outputs.

## HTTP Routes

| Method | Path | Body |
| --- | --- | --- |
| `GET` | `/v1/datasets` | None. |
| `GET` | `/v1/datasets/{reference}` | None. |
| `DELETE` | `/v1/datasets/{reference}` | None. |
| `POST` | `/v1/datasets/import` | `{"path":"/absolute/dataset-path"}` |
| `POST` | `/v1/datasets/import/jobs` | Same as `/v1/datasets/import`, returns a job. |
| `POST` | `/v1/datasets/validate` | `{"path":"optional-path","dataset_ref":"optional-ref"}` |
| `POST` | `/v1/datasets/template` | `{"task":"optional-task","language":"optional-language"}` |
| `POST` | `/v1/datasets/{reference}/export` | `{"output_path":"/absolute/output-path"}` |
| `POST` | `/v1/datasets/{reference}/diff` | `{"right_dataset_ref":"optional-ref","right_path":"optional-path"}` |
| `POST` | `/v1/datasets/synth/jobs` | Provider-backed dataset synthesis job through OpenAI, Anthropic, or Gemini. |
| `POST` | `/v1/datasets/eval/jobs` | Provider-backed dataset evaluation job through OpenAI, Anthropic, or Gemini. |

## Related Guides

[LoRA training](./training-lora.md) uses managed training splits.
[Jobs](./jobs.md) explains cloud generation/evaluation status and outputs;
[authentication](./auth.md) controls provider credentials.
