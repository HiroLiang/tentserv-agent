# Model Support Catalog

Tentgent ships a built-in model support catalog for well-known Hugging Face
model families and the small fixture models used by local smoke tests.

The catalog is local data. It does not call Hugging Face, NVIDIA, or any model
provider during `model inspect`.

## What It Means

The catalog helps Tentgent identify a model source and explain likely endpoint
families before a local proof exists.

Catalog entries may describe:

- publisher and model family, such as Qwen, Llama, Gemma, Nemotron, or
  `mlx-community`;
- model scale, such as 0.5B, 70B, 120B, 235B, or larger MoE variants;
- Tentgent endpoint capabilities, such as `chat`, `embedding`, `rerank`,
  `vision-chat`, `audio-transcription`, `audio-speech`, and
  `image-generation`;
- descriptive tags such as `reasoning`, `tool-use`, `code`, `multimodal`,
  `mlx-lm`, or `nvidia-nim-recommended`.

The catalog is not local verification proof. A large model may be known by the
catalog and still require external serving infrastructure.

## Support Levels

| Level | Meaning |
| --- | --- |
| `fixture-supported` | A small model pinned by Tentgent fixture docs. It can produce a `supported` hint before local proof. |
| `local-runtime-supported` | A known local-runtime family, such as an MLX conversion pattern, can produce a `supported` hint when metadata matches. |
| `catalog-known` | Tentgent knows the model family and likely endpoint family, but does not treat it as locally supported without proof. |
| `requires-external-runtime` | The model is known but usually needs external GPU, NIM, or other serving infrastructure before Tentgent can use it locally. |
| `known-unsupported` | The catalog has a negative record for the current model/capability family. |
| `deprecated` | Kept for recognition, not recommended for new local workflows. |

Only `fixture-supported`, `local-runtime-supported`, and `known-unsupported`
entries become support hints for the status resolver. `catalog-known` and
`requires-external-runtime` entries are displayed by `model inspect`, but they
do not become local support proof.

## Inspect Output

List catalog entries before pulling a model:

```bash
tentgent model catalog
tentgent model catalog --capability chat --publisher Qwen
tentgent model catalog --support-level fixture-supported
tentgent model catalog --local --capability embedding
tentgent model catalog --query nemotron
```

Available filters:

- `--capability`: match a Tentgent endpoint capability.
- `--publisher`: case-insensitive publisher text match.
- `--support-level`: match one catalog support level.
- `--local`: show only entries that can become local support hints.
- `--query`: search publisher, family, source, tags, and recommendation text.

After the table, the command prints this pull template:

```bash
tentgent model pull <publisher>/<source> --capability <capability>
```

It also prints descriptions for the capabilities present in the filtered
results. Pattern rows such as `mlx-community/Qwen*` are recognition rules; use
a concrete Hugging Face repository when filling the template.

Use:

```bash
tentgent model inspect <model-ref-or-prefix>
```

`model inspect` shows a `catalog` row when a built-in entry matches the stored
source metadata. The capability support rows remain proof-aware:

- `verified` and `failed` come from local proof records.
- `supported` or `unsupported` can come from built-in support hints.
- `unknown` means the model declares a capability but no applicable proof or
  support hint exists.

Local proof always wins over catalog hints. For example, if a curated fixture
has a built-in `supported` hint but the latest local smoke test failed, inspect
shows `failed` with `evidence: local-proof`.

`model ls`, `model inspect`, `server inspect`, and `doctor` surface these
statuses for visibility. In this release, warnings for unknown, stale,
unsupported, or failed tuples do not automatically block existing commands.

Use the status as the next-action hint:

- `verified`: prefer this local tuple.
- `supported`: try it or run a local smoke verification when needed.
- `failed`: inspect the proof, fix the runtime/backend/input issue, then retry
  verification.
- `unsupported`: choose another model, capability, backend, or route.
- `unknown`: add support evidence or explicitly allow and verify the tuple.
- `stale`: rerun verification under the current runtime/profile/platform.

## Scope

The first built-in catalog covers representative models from:

- Hugging Face fixture publishers used in Tentgent docs;
- Meta Llama and Code Llama families;
- Qwen chat, coder, VL, audio/omni, embedding, rerank, and image families;
- Google Gemma, PaliGemma, MedGemma, and ShieldGemma families;
- NVIDIA Nemotron, Llama Nemotron, retrieval, rerank, multimodal, and safety
  families;
- `mlx-community` conversion patterns for Qwen, Llama, Gemma, Mistral, VLM,
  audio, TTS, and diffusion models;
- common embedding, rerank, Stable Diffusion, and Flux families.

The catalog is intentionally conservative. It identifies likely model families;
it does not imply every derivative, quantization, or revision has been verified
on the current machine.
