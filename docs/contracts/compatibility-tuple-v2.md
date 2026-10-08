# Compatibility Tuple And Proof v2

This is the #127 kernel foundation contract. It does not switch existing
Local/Cluster gates to v2, collect runtime facts, introduce public query flags,
or authorize additional backend workflows. Those integrations belong to
#128/#129/#130. The [proof schema router](./model-support-proof-schema.md)
defines legacy compatibility and producer timing.

## Complete Identity

`CompatibilityTuple` contains `identity_version = 1` and a validated
`components` object. All fields below are required; absence must be explicit
where the type allows it. Construction and JSON/TOML deserialization enforce
the same validation and reject unknown fields.

| Component | Representation |
| --- | --- |
| `model_ref` | Canonical managed content reference. Source names and short refs are not substitutes. |
| `capability` | Existing model capability enum. |
| `primary_format` | Existing model format enum. |
| `quantization` | `kind: unquantized`, or `kind: quantized` with registered `method`, opaque `variant`, optional `bits` and `group_size`. |
| `backend`, `runtime_family` | Registered labels with a validated backend/family/package combination. |
| `runtime` | Selected backend `package` and observed `version`. |
| `profile` | `kind: no-profile`, or `kind: selected` with execution-profile `id` and positive `version`. |
| `platform` | Registered `os` and `architecture`. |
| `device_class` | `cpu`, `cuda`, or `metal`. |
| `adapter` | `kind: base`, or `kind: selected` with both `adapter_ref` and opaque `load_identity`. |
| `observation` | `kind: load`, or `kind: execution` with validated `input` and `output` shapes. |

Explicit base-model, no-profile, and unquantized values mean those facts were
established. They do not mean unknown, unavailable, or omitted. Selected
adapter identity is supplied by the future serving integration; #127 does not
hash adapter weights or decide scale/configuration identity itself.

Runtime identity is the selected or reused worker's backend distribution,
not the Tentgent CLI version, Python project version, caller's environment,
or bootstrap dependency profile. No fact collector or missing-field backfill
is implied by accepting a complete tuple from a producer.

## Observation Is Not Load Mode

The observation kind is part of the hashed identity. It describes what was
observed, not whether the server was started with lazy/eager `LoadMode`.

| Proof source | Permitted v2 observation |
| --- | --- |
| `server-start` | `load` only: terminal eager preload outcome. |
| `runtime-execution` | `execution` only: actual dispatched work. |
| `endpoint-smoke` | `execution` only: the exercised input/output shape. |
| `manual-probe` | Rejected for v2: the current operation is metadata-only. |

A load observation has no fabricated input/output shape. Even a complete
successful load proof cannot authorize inference, streaming, adapter serving,
provider formatting, or all image workflows. Changing only the observation
from load to execution changes the tuple key.

Execution shapes contain:

- Input: `family`, nonempty set of `modalities`, `provider`, and `attributes`.
- Output: `family`, nonempty set of `modalities`, explicit `streaming`, and
  `format`.
- Both families must equal the tuple capability. Modalities are sorted and
  deduplicated in canonical enum order: text, image, audio, video.
- Provider labels are `native`, `openai`, `claude`, and `gemini`.
- Output formats are `text`, `json`, `float-vector`, `ranked-documents`, `wav`,
  `pcm`, `mp3`, `png`, and `jpeg`.
- Attributes are the fixed allowlist `tool_calls`, `structured_output`,
  optional `image_workflow`, and optional `embedding_input`. The first two
  booleans are explicit, including false.
- Tool/structured flags are limited to chat, vision-chat, and
  video-understanding. Image workflow is limited to image-generation;
  embedding input is limited to embedding.
- Image workflow values are `text-to-image`, `image-to-image`, `inpainting`,
  and `controlnet`. Embedding input values are `query` and `document`.

Representability is not a runtime support claim. The tuple type does not make
a missing provider/backend operation executable. Prompts, source files,
filenames, headers, arbitrary attribute maps, ordinary sampling values, and
request-size buckets are not shape fields. Verified does not promise that
every input size fits memory or succeeds.

## Registered Values And Bounds

Registered labels canonicalize only their declared case-insensitive aliases.
Opaque identifiers and versions preserve case and never trim or guess values.

| Runtime family | Backend | Selected package |
| --- | --- | --- |
| `mlx-lm` | `mlx` | `mlx-lm` |
| `mlx-vlm` | `mlx` | `mlx-vlm` |
| `mlx-audio` | `mlx` | `mlx-audio` |
| `mlx-diffusion` | `mlx` | `mflux` |
| `transformers` | `transformers` | `transformers` |
| `llama-cpp` | `llama-cpp` | `llama-cpp-python` |
| `diffusers` | `diffusers` | `diffusers` |

- OS: `macos` (`darwin` alias), `linux`, `windows` (`win32` alias).
- Architecture: `aarch64` (`arm64` alias), `x86_64` (`amd64` alias).
- `mps` canonicalizes to device class `metal`; `llama_cpp` and
  `llama_cpp_python` are registered aliases for their hyphenated labels.
- Quantization methods: `mlx`, `gguf`, `bitsandbytes`, `gptq`, `awq`.
  If present, bits are 1-64 and group size is positive.
- Opaque identifiers: 1-128 ASCII bytes, alphanumeric or `._-:/+`.
  Empty, `unknown`, `latest`, `n/a`, and `not-applicable` are rejected.
- Runtime version: 1-128 ASCII bytes, alphanumeric or `._+-!`, with at least
  one digit. A syntactically valid value still requires authoritative producer
  provenance; parsing is not evidence that the package was installed or used.
- Selected profile version is greater than zero.

This first identity uses OS/architecture/device class and the selected backend
package version, not every transitive dependency, driver, OS patch, GPU model,
or memory limit. Stronger environment fingerprints require a new reviewed
contract; never imply full environment reproducibility from this tuple.

## Canonical Key

The key is lowercase hexadecimal SHA-256 of the tuple's compact UTF-8 JSON,
including its identity version. Serialization uses the typed field order:
`identity_version`, then `components`; within components, the order is the
table above. Nested structures also have fixed typed field order. Golden
vectors freeze this encoding; it is not an arbitrary map serialization.

Do not hash status, source, timestamp, error text, server ref, runtime
generation token, load mode, or either idle timeout. Any identity normalization
or canonical encoding change requires a new identity version. Existing
partial filename escaping is not reused for v2.

The persisted location is:

```text
models/store/<model_ref>/support-proofs/v2/<capability>/<tuple_sha256>.toml
```

A key has exactly 64 lowercase hexadecimal characters. Validate the directory
model and capability, filename digest, and body tuple together. There is one
current file per exact tuple; the last successfully committed serialized
replacement is current. Timestamps are provenance, not interprocess ordering
or compare-and-swap authority. No persistent secondary index is introduced.

## Proof Envelope

Required fields are `schema_version = 2`, `record_kind = "local-proof"`,
`tuple`, `status`, `source`, and `checked_at`. The status is `verified` or
`failed`; time is valid RFC3339, at most 64 bytes. A failed record requires a
fixed `failure_code`; a verified record must not carry one.

Failure codes are `model-load-failed`, `runtime-execution-failed`,
`resource-exhausted`, `unsupported-execution`, and `invalid-runtime-response`.
They map to fixed safe summaries. V2 has no arbitrary exception string field.
The store bounds persisted v2 input to 16 KiB before parsing. Unsupported
schema/identity versions, unknown fields, invalid values, and body/key
mismatch return explicit errors; they do not become verified results.

This synthetic TOML illustrates the wire shape, not evidence of a real run:

```toml
schema_version = 2
record_kind = "local-proof"
status = "verified"
source = "server-start"
checked_at = "2026-10-08T00:00:00Z"

[tuple]
identity_version = 1

[tuple.components]
model_ref = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
capability = "chat"
primary_format = "safetensors"
backend = "transformers"
runtime_family = "transformers"
device_class = "cpu"

[tuple.components.quantization]
kind = "unquantized"

[tuple.components.runtime]
package = "transformers"
version = "4.50.0"

[tuple.components.profile]
kind = "no-profile"

[tuple.components.platform]
os = "linux"
architecture = "x86_64"

[tuple.components.adapter]
kind = "base"

[tuple.components.observation]
kind = "load"
```

## Evidence, Queries, And Compatibility

The evidence view preserves `v2`, `tuple-aware-v1`, or `legacy-latest`
generation, source, status, time, present facts, missing dimensions, and the v2
key when available. Legacy evidence in this new view uses a fixed failed-proof
summary, not arbitrary historical exception text. Existing legacy API views
retain their bounded, sanitized diagnostic summaries.

Typed filters enumerate a model's files in memory. A partial filter match
cannot authorize execution; exact resolution compares the complete tuple and
observation. Missing old dimensions remain stale evidence. A current exact v2
record outranks less-specific old evidence; unrelated newer proof cannot
override it. Hard incompatibility and applicable failure precedence remain.

V2 and old files coexist without backfill or v2-to-legacy projection. Existing
gate adapters keep their old behavior until #128/#129. Exact removal deletes
one v2 record; capability-wide clear removes all generations under one short
transaction and counts logical evidence, not mirrors. No tombstone protocol
is required because old evidence cannot regain exact authority.

An old binary ignores v2 and its clear operation leaves v2 untouched;
re-upgrade can expose those records again. Concurrent old/new writers,
network filesystems, and different runtime homes sharing one data root are
outside the coordination guarantee. Per-file atomic replacement does not
promise multi-file crash rollback. See
[proof transactions](./resource-blockers.md#proof-transactions) and
[persistence failure semantics](./model-support-proof-schema.md#persistence-and-clearing).

This foundation does not alter #131's idle clocks, first-spawner policy or
ownership generations, or #132's worker-owned readiness/proof timing,
Cluster reload/drain, Cloud restrictions, and image lazy-only guard.
