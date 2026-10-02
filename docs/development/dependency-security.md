# Dependency Security Review

This is a release-review record, not a claim that the dependency graph is free
of vulnerabilities. Frozen Python installs use the workspace `uv.lock`; review
all platform and optional-dependency resolutions, not only the developer's
installed environment.

## 2026-10-03 Rust Review

All **313 registry package/version pairs** in `Cargo.lock` were queried against
the [official OSV batch API](https://google.github.io/osv.dev/api/#operation/OSV_QueryBatch).
The initial affected packages were updated as follows:

| Package | Before | After | Advisory |
| --- | --- | --- | --- |
| quinn-proto | 0.11.14 | 0.11.15 | [GHSA-4w2j-m93h-cj5j](https://github.com/quinn-rs/quinn/security/advisories/GHSA-4w2j-m93h-cj5j) |
| rpassword | 7.4.0 | 7.5.4 | [GHSA-2p6r-x3vv-xqm2](https://github.com/advisories/GHSA-2p6r-x3vv-xqm2) |
| rustls | 0.23.38 | 0.23.45 | [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html) |
| rustls-webpki | 0.103.12 | 0.103.15 | [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104.html) |
| time | 0.3.44 | 0.3.47 | [RUSTSEC-2026-0009](https://rustsec.org/advisories/RUSTSEC-2026-0009.html) |

The final exact-version OSV query returned no affected packages among those
313 pairs. This describes known advisory data at review time, not proof of no
vulnerabilities. The attempted cargo-audit 0.22.2 installation stalled while
updating the registry and was stopped; it is **not** recorded as a passed audit.

rpassword 7.5.0 initially failed to compile on macOS because it referenced a
Linux-only errno symbol; 7.5.4 fixes that compatibility failure. The integrated
Rust suite then passed 922 tests, with 9 explicitly ignored tests. Native release
target builds remain required in addition to the local suite and advisory scan.

## 2026-10-03 Python Review

The initial installed-environment audit reported 79 advisory rows across 12
packages, including duplicate records (45 unique advisory IDs). Security floors
and the lock were updated with the smallest compatible patched release line:

| Package | Locked version | Reason |
| --- | --- | --- |
| aiohttp | 3.14.3 | HTTP parser, cookie, and client security fixes |
| anyio | 4.14.2 | TLS hostname verification fixes |
| datasets | 5.0.1 | Metadata file path traversal fix |
| diffusers | 0.38.0 | Model repository code-loading fixes |
| pillow | 12.3.0 | Image/font parser and resource-limit fixes |
| setuptools | 83.0.0 | Source manifest path exclusion fix |
| starlette | 1.3.1 | Request parsing, redirect, and file-serving fixes |
| torch | 2.13.0 | Script compiler memory-safety fix |
| torchvision | 0.28.0 | Requires the matching `torch==2.13.0` |
| transformers | 5.10.4 | Patched 5.10 line; 5.10.0 was yanked |
| urllib3 | 2.8.0 | Streaming parser, decompression, and proxy TLS fixes |

Required transitive updates are safetensors 0.8.0 (Diffusers requirement),
cuda-toolkit 13.0.3.0, and triton 3.7.1 (Torch resolution). Other dependencies
were not indiscriminately upgraded. Workspace constraints protect transitive
security floors; runtime metadata also declares its direct dependency floors.

`pip-audit` covered all **171 registry package/version pairs** in the universal
lock: 170 in the main audit and a separate audit of `fsspec==2026.2.0`. The latter
is necessary because the lock contains two marker-selected fsspec versions and
`pip-audit` accepts only one version per package per invocation. The union was
compared with the lock: no missing or extra pairs. No advisory was ignored.

The updated audit still reports three rows representing the two unique,
unpatched advisories below. It intentionally exits nonzero. These findings must
remain visible in release review rather than being relabeled as a clean scan.

## Unpatched Findings And Application Boundaries

### Accelerate: checkpoint shard paths

[GHSA-4j2p-28q2-5m79](https://github.com/advisories/GHSA-4j2p-28q2-5m79)
affects `load_checkpoint_in_model` and `load_checkpoint_and_dispatch`: untrusted
checkpoint `weight_map` entries can escape the model directory or reference
named pipes. No patched release was available at review time. The dependency
remains installed for backend dispatch and training support.

Tentgent does not call those two loaders directly. This alone is not a durable
mitigation: transitive loaders may change. The managed model preflight in
`backends/model_safety.py` must reject absolute/traversing shard references,
symlink escapes, and nonregular files before model or adapter loading. This is
a validation boundary, not a sandbox against a local OS user who can replace
trusted assets after validation.

The shared ResourceManager preflight covers initial inference and preload;
MLX training also checks assets before starting its subprocess. The 34 dedicated
asset-safety regressions passed, including malicious shard paths, symlinks,
FIFOs, bounded configuration parsing, and accepted ordinary models. The focused
guard/resource/lifecycle/preload run passed 100 tests and 7 subtests; the combined
asset and loader-trust suite passed 46 tests.

Adapter and ControlNet entry points now validate before native loading; the
loader-policy suite passed 13 tests, including repeat rejection after a failed
Diffusers adapter setup. The full Python 3.12 suite passed 178 tests and 11
subtests. Real MLX lifecycle validation passed all three cases in 48.2 seconds,
including idle release, process exit, reuse, and restart. Track the
[upstream fix](https://github.com/huggingface/accelerate/pull/4138) and remove the
exception only after auditing a patched lock.

### Diskcache: pickle-backed cache reads

[GHSA-w8v5-vhqr-4h9v](https://github.com/advisories/GHSA-w8v5-vhqr-4h9v)
allows code execution when an attacker can write a pickle payload into a cache
directory that the application subsequently reads. No patched release was
available. Diskcache 5.6.3 is a dependency of llama-cpp-python 0.3.23.

Tentgent constructs `llama_cpp.Llama` directly, whose default cache is `None`;
it neither enables `LlamaDiskCache` nor calls `set_cache`. It does not launch
llama-cpp-python's separate server, which can configure that cache. Therefore
the vulnerable persistence path is not enabled in the current application.
Keep it disabled. Any future disk-cache feature requires a separate security
decision and must not trust attacker-writable serialized content.

Release gate: retain evidence that the locked llama-cpp-python default remains
`None` and no Tentgent loader enables persistent caching. Do not hide the
dependency advisory or treat it as an upstream fix.

## Interpreter Compatibility Gate

The dependency check found a pre-existing mismatch: Misaki 0.9.4, required by
the audio/full local-model profiles, declares `>=3.8,<3.13`, while the managed
bootstrap selected Python 3.13. Its latest published release still has that
restriction ([official metadata](https://pypi.org/pypi/misaki/0.9.4/json)). Frozen
installation succeeded but `uv pip check` correctly rejected that environment.

The supported source range is now **Python 3.11–3.12** in both manifests, and
managed installation selects **3.12**. Do not modify third-party metadata to
silence the check, silently omit the speech dependency, or claim that the old
3.13 environment is valid. A fresh Python 3.12.13 full-profile environment passed
`uv pip check` (150 packages), imports of the runtime and all selected native
backend families including Misaki, and all 178 Python tests plus 11 subtests.
This does not change the machine's global Python installations. The separate
MLX real-model smoke passed; native release runners and installed artifact
tests remain additional gates, not implied by this local environment.

## Reproduce The Universal Audit

Run from the repository root. These commands query vulnerability metadata;
they do not sync the runtime environment. Keep generated JSON under ignored
`test-data/`, and preserve it with release evidence.

```bash
uv lock --check
uv export --format requirements-txt --all-extras --all-packages \
  --no-hashes --no-emit-workspace --frozen --no-header \
  | sed -e 's/ ; .*//' -e '/^[[:space:]]*#/d' \
      -e '/^fsspec==2026\.2\.0$/d' \
  | sort -u \
  | uvx pip-audit --requirement /dev/stdin --no-deps --disable-pip \
      --strict --format json
printf '%s\n' 'fsspec==2026.2.0' \
  | uvx pip-audit --requirement /dev/stdin --no-deps --disable-pip \
      --strict --format json
```

Recheck duplicate package versions and coverage whenever the lock changes;
the fsspec split is specific to this review. Also run `uv pip check`, runtime
imports, the full Python tests, and real-model smoke after syncing the lock.
Resolution and vulnerability metadata alone do not prove backend compatibility.

Selected primary evidence:
[aiohttp](https://github.com/aio-libs/aiohttp/security/advisories/GHSA-cq5v-8q36-5273),
[AnyIO](https://github.com/agronholm/anyio/security/advisories/GHSA-82r6-8w77-94w6),
[Datasets](https://github.com/huggingface/datasets/pull/8325),
[Starlette](https://github.com/encode/starlette/security/advisories/GHSA-82w8-qh3p-5jfq),
[Transformers release metadata](https://pypi.org/pypi/transformers/5.10.4/json),
[Torchvision requirements](https://pypi.org/pypi/torchvision/0.28.0/json).
