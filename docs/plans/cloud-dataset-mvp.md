# Cloud Dataset MVP

This plan defines the next active track: use existing provider auth to help generate, validate, and evaluate Tentgent datasets without mixing cloud APIs into local model runtime ownership.

## Scope

- Reuse existing `auth openai` and `auth anthropic` secret resolution.
- Generate `tentgent.chat.v1` dataset records for local LoRA training.
- Keep OpenAI and Claude as dataset/evaluation providers, not managed local models.
- Keep the first implementation review-sized and file-based.

## Non-Goals

- Do not add cloud providers to the model store in this phase.
- Do not start cloud-backed `tentgent server` instances yet.
- Do not automatically train after dataset generation.
- Do not auto-publish generated datasets or adapters.

## Command Surface

Planned first commands:

```text
tentgent dataset validate <PATH>
tentgent dataset synth --provider <openai|anthropic> --output <DIR> [OPTIONS]
tentgent dataset eval <DATASET_REF|PATH> --provider <openai|anthropic> [OPTIONS]
```

## First Slice

Implement `dataset validate <PATH>` first.

Goals:

- validate local files against `tentgent.chat.v1`
- report train/valid/test/eval split counts
- surface schema errors with file and line number
- avoid network calls

## Second Slice

Implement a file-first `dataset synth` draft.

Goals:

- resolve provider keys through existing auth infrastructure
- write generated JSONL into an output directory
- do not automatically run `dataset add`
- print the suggested `tentgent dataset add <DIR>` command

## Third Slice

Implement `dataset eval` for generated or managed datasets.

Goals:

- evaluate whether answers stay inside provided context
- flag language mismatch, hallucination, unsafe requests, and format drift
- write a local evaluation report rather than mutating the dataset

## Dataset Contract

All generated records must use [../contracts/dataset-schema.md](../contracts/dataset-schema.md).

Providers must output JSONL where each row has:

- `schema = "tentgent.chat.v1"`
- `messages`
- optional `tools`
- optional `metadata`

Generated data must not be pre-rendered as MLX, PEFT, ChatML, or provider-specific prompt text.

## Open Questions

- Should `dataset synth` accept a plain brief, a Markdown spec file, or both?
- Should generated output include separate `train.jsonl`, `valid.jsonl`, and `test.jsonl` by default?
- Should provider/model choices be stored in `manifest.json` before `dataset add`?
