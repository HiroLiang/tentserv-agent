# Dataset Store

This document defines the dataset-store boundary for Tentgent training and evaluation workflows.

The current implementation supports local dataset validation, manual generation
templates, local dataset imports, deterministic manifests, content-derived
references, deduplication, split detection, safe export to working directories,
listing, and inspection. Provider-backed `dataset synth` and `dataset eval` are
paused until their runtime is ported to the model runtime HTTP boundary.

## Command Shape

Implemented command group:

```text
tentgent dataset add <PATH>
tentgent dataset validate <PATH>
tentgent dataset template [-t|--task <KIND>] [-l|--language <LANG>] [-o|--output <PATH>]
tentgent dataset ls
tentgent dataset inspect <DATASET_REF>
tentgent dataset export <DATASET_REF> <PATH>
tentgent dataset diff <LEFT_REF> <RIGHT_REF>
tentgent dataset diff <LEFT_REF> [-p|--path <PATH>]
tentgent dataset rm <DATASET_REF>
```

Planned future commands may add hosted dataset imports and richer evaluator presets.

## Supported Inputs

The local import path accepts:

- a single `.jsonl` file
- a directory containing dataset files

The canonical chat and tool-use schema is defined in [dataset-schema.md](./dataset-schema.md). The store remains responsible for content identity, layout, and indexes; schema validation and backend rendering are separate concerns.

Use `dataset validate <PATH>` to check local JSONL files or dataset directories against the canonical schema before import. Use `dataset template` to print or write a deterministic prompt that asks OpenAI, Claude, or another agent to produce `tentgent.chat.v1` JSONL.

Provider-backed `dataset synth` and `dataset eval` should be restored as HTTP
model-runtime endpoints before they are advertised as implemented commands
again.

## Template Source

Dataset prompt templates should live as editable Markdown under
`src/tentgent-kernel/src/features/dataset/templates/`. Rust renderers may
perform small placeholder substitutions, but long prompt bodies should remain in
`.md` files so dataset generation and evaluation wording can be reviewed without
digging through service logic.

## Training Package Shape

The minimum tuning-ready package is:

```text
<dataset-dir>/
└── train.jsonl
```

Recommended package shape:

```text
<dataset-dir>/
├── train.jsonl
├── valid.jsonl
├── test.jsonl
├── eval_cases.jsonl
└── manifest.json
```

Split semantics:

- `train.jsonl` is required for future tuning commands.
- `valid.jsonl` is optional and is used for validation loss during training.
- `test.jsonl` is optional and is reserved for held-out trainer evaluation.
- `eval_cases.jsonl` is optional and belongs to Tentgent behavior evaluation, not direct trainer input.
- `manifest.json` is optional source metadata from the dataset author, separate from Tentgent's generated store `manifest.json`.

Compatibility notes:

- MLX local LoRA training expects `train.jsonl`, optional `valid.jsonl`, and `test.jsonl` for test runs.
- PEFT/TRL training can use the same JSONL content after loading through Hugging Face `datasets`.
- `val.jsonl` is treated as a legacy validation alias. If both `val.jsonl` and `valid.jsonl` exist, `valid.jsonl` wins and a warning is recorded.

## Layout

```text
TENTGENT_HOME/
└── datasets/
    ├── store/
    │   └── <dataset_ref>/
    │       ├── dataset.toml
    │       ├── manifest.json
    │       └── source/
    ├── by-source/
    │   └── local/
    │       └── <dataset_ref>.toml
    └── staging/
```

## Metadata

`dataset.toml` includes:

- `dataset_ref`
- `short_ref`
- `source_kind = "local" | "generated" | "huggingface"`
- `source_path` for local imports
- `source_repo` for future Hugging Face imports
- `source_revision` for future Hugging Face imports
- `dataset_format`
- `package.tuning_ready`
- `package.splits.train`
- `package.splits.validation`
- `package.splits.test`
- `package.splits.eval_cases`
- `package.splits.source_manifest`
- `package.warnings`
- `file_count`
- `total_bytes`
- `imported_at`

Future training-oriented fields may include:

- `task_kind`
- `schema_kind`
- `license`
- `generated_by_provider`
- `generated_by_model`
- `parent_dataset_ref`

## Identity Rule

The dataset identity is content-derived, not source-name-derived.

Implemented rule:

- build a deterministic manifest of all regular source files
- hash normalized relative paths, file sizes, and per-file SHA-256 values
- use `dataset_ref = sha256(canonical_manifest_json_bytes)`
- use `short_ref = first 12 hex chars of dataset_ref`

For a single `.jsonl` file, the original filename is part of the normalized manifest path after the file is copied into `source/`. Two files with identical bytes but different filenames are therefore different dataset layouts in this MVP.

## Deduplication

If `datasets/store/<dataset_ref>` already exists, `dataset add` reuses the existing managed dataset and refreshes the local source index instead of copying data again.

Canonical ownership always lives under `datasets/store/<dataset_ref>`. `by-source/local/<dataset_ref>.toml` is lookup metadata only.

## Working Copies

Managed dataset sources are content-addressed and should be treated as immutable.

Use `dataset export <DATASET_REF> <PATH>` to copy `store/<dataset_ref>/source/` into a local working directory. The destination is created if missing. If the destination already exists, it must be an empty directory.

After editing the exported working copy, run `dataset add <PATH>` to create a new content-derived `dataset_ref`.

## Dataset Diff

Use `dataset diff <LEFT_REF> <RIGHT_REF>` to compare two managed dataset versions.

Use `dataset diff <LEFT_REF> --path <PATH>` to compare one managed dataset version against a local working copy.

Diff compares manifest entries and reports:

- `added`: files present only in the right dataset
- `removed`: files present only in the left dataset
- `modified`: files with the same normalized path but different size or SHA-256
- `unchanged`: files with the same normalized path, size, and SHA-256

For `--path`, the local path is treated as the right side. Tentgent computes its manifest temporarily and does not write it into the managed store.

## Dataset Eval Reports

Provider-backed eval reports are paused until the evaluator runtime is ported to
the model runtime HTTP boundary. When restored, the report directory must be
missing or empty and contain:

- `eval-report.json`: structured `tentgent.dataset.eval.report.v1` report
- `eval-report.md`: human-readable summary
- `prompt.md`: exact provider evaluation prompt
- `provider-output.raw.txt`: raw provider response

The evaluator supports `--split train|valid|test|eval_cases|all`, `--max-records <N>`, and `--criteria <TEXT>`. Criteria are useful for project-specific style checks, such as whether final assistant replies follow a desired verbal habit.

## Removal

Use `dataset rm <DATASET_REF>` to remove one managed dataset store record and its local source index.

Removal does not delete exported working copies. Kernel-backed dataset removal
checks local LoRA train plans and run records before deleting managed dataset
content. The legacy HTTP dataset routes still await the kernel migration.

## Non-Goals

- no Hugging Face dataset pull
- no training integration
