#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"

fail() {
  echo "test-package-python-layout: $*" >&2
  exit 1
}

command -v uv >/dev/null 2>&1 || fail "uv is required"
[[ -f "${root_dir}/uv.lock" ]] || fail "repository uv.lock is missing"

temp_root="$(mktemp -d /tmp/tentgent-package-layout.XXXXXX)"
trap 'rm -rf "${temp_root}"' EXIT

share_dir="${temp_root}/share/tentgent"
project_dir="${share_dir}/python/tentgent-model-runtime"
env_dir="${temp_root}/runtime/python-env"
uv_cache_dir="${temp_root}/runtime/bootstrap/uv-cache"

mkdir -p "${project_dir}" "${share_dir}/scripts"
cp "${root_dir}/pyproject.toml" "${share_dir}/pyproject.toml"
cp "${root_dir}/uv.lock" "${share_dir}/uv.lock"
cp "${root_dir}/scripts/bootstrap-python-env.sh" "${share_dir}/scripts/bootstrap-python-env.sh"
cp "${root_dir}/scripts/bootstrap-uv.sh" "${share_dir}/scripts/bootstrap-uv.sh"

tar \
  --exclude='.venv' \
  --exclude='__pycache__' \
  --exclude='*.pyc' \
  --exclude='.pytest_cache' \
  --exclude='.ruff_cache' \
  --exclude='.mypy_cache' \
  --exclude='.DS_Store' \
  -C "${root_dir}/python/tentgent-model-runtime" \
  -cf - . | tar -C "${project_dir}" -xf -

[[ -f "${project_dir}/pyproject.toml" ]] || fail "packaged runtime pyproject.toml is missing"
[[ -d "${project_dir}/src" ]] || fail "packaged runtime src directory is missing"

TENTGENT_HOME="${temp_root}" \
TENTGENT_BOOTSTRAP_UV_CACHE_DIR="${uv_cache_dir}" \
  bash "${share_dir}/scripts/bootstrap-python-env.sh" \
    --project "${project_dir}" \
    --env "${env_dir}" \
    --uv "$(command -v uv)" \
    --dry-run

echo "package Python layout tests passed"
