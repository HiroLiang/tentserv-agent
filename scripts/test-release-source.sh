#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${root_dir}"

validation_env="$(mktemp -d "${TMPDIR:-/tmp}/tentgent-release-python.XXXXXX")"
trap 'rm -rf "${validation_env}"' EXIT
export UV_PROJECT_ENVIRONMENT="${validation_env}/env"
if command -v cygpath >/dev/null 2>&1; then
  UV_PROJECT_ENVIRONMENT="$(cygpath -m "${validation_env}/env")"
fi

# Run on the actual package host before signing or publishing an artifact.
case " ${RUSTFLAGS:-} " in
  *" -D warnings "*) ;;
  *) export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-D warnings" ;;
esac
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo build --locked -p tentgent-cli -p tentgent-daemon --bins

# uv's local-project cache need not notice source-only changes when package
# metadata is unchanged. Build this checkout again, not a cached 0.1.0 wheel.
uv sync --project python/tentgent-model-runtime --frozen --no-editable \
  --reinstall-package tentgent-model-runtime --group dev --python 3.12
uv pip check --python "${UV_PROJECT_ENVIRONMENT}"
uv run --no-sync --project python/tentgent-model-runtime python -I scripts/check-release-python-source.py
uv run --no-sync --project python/tentgent-model-runtime python -c \
  'from tentgent.runtime.server.app import create_app; print("Python runtime import passed")'
uv run --no-sync --project python/tentgent-model-runtime python -m pytest \
  python/tentgent-model-runtime/tests -q -ra
uv run --no-sync --project python/tentgent-model-runtime python scripts/test-installed-release-unit.py
uv run --no-sync --project python/tentgent-model-runtime python scripts/test-release-source-unit.py

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    # The POSIX fixtures use shell executables and are not Windows evidence.
    pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/test-windows-runtime-upgrade.ps1
    bash scripts/test-release-metadata.sh
    ;;
  Darwin|Linux)
    for suite in local-server-startup cluster-server-startup cluster-server-reload; do
      uv run --no-sync --project python/tentgent-model-runtime python "scripts/test-${suite}.py"
    done
    bash scripts/test-release-readiness.sh
    ;;
  *) echo "unsupported native release test host" >&2; exit 1 ;;
esac
