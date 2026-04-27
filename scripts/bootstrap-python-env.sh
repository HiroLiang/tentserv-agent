#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_PYTHON_VERSION="3.13"

usage() {
  cat <<'USAGE'
Usage: scripts/bootstrap-python-env.sh [OPTIONS]

Create or sync Tentgent's managed Python runtime environment using the pinned
uv executable from the Tentgent bootstrap cache.

This is installer-owned bootstrap plumbing. Normal runtime commands should use
the generated tentgent-* entry points, not uv.

Options:
  --project <PATH>  Python daemon project directory. Defaults to packaged or repo project.
  --env <PATH>      Managed Python environment path. Defaults to TENTGENT_HOME/runtime/python-env.
  --uv <PATH>       Use an explicit pinned uv executable path.
  --dry-run         Ask uv to plan the sync without modifying the environment.
  --print-plan      Print resolved paths without syncing.
  -h, --help        Show this help.

Environment:
  TENTGENT_HOME                       Override Tentgent runtime home.
  TENTGENT_PYTHON_DIR                 Override Python daemon project directory.
  TENTGENT_PYTHON_ENV_DIR             Override managed Python environment path.
  TENTGENT_BOOTSTRAP_UV               Override pinned uv executable path.
  TENTGENT_BOOTSTRAP_UV_CACHE_DIR     Override uv package/cache directory.
  TENTGENT_BOOTSTRAP_PYTHON_VERSION   Override managed Python version. Defaults to 3.13.
USAGE
}

fail() {
  echo "error: $*" >&2
  exit 1
}

default_runtime_home() {
  if [[ -n "${TENTGENT_HOME:-}" ]]; then
    echo "${TENTGENT_HOME}"
    return
  fi

  case "$(uname -s)" in
    Darwin) echo "${HOME}/Library/Application Support/com.tentserv.tentgent" ;;
    Linux) echo "${XDG_DATA_HOME:-${HOME}/.local/share}/tentgent" ;;
    *) fail "TENTGENT_HOME is required on this platform" ;;
  esac
}

resolve_python_project() {
  if [[ -n "${PROJECT_DIR_OVERRIDE}" ]]; then
    echo "${PROJECT_DIR_OVERRIDE}"
    return
  fi
  if [[ -n "${TENTGENT_PYTHON_DIR:-}" ]]; then
    echo "${TENTGENT_PYTHON_DIR}"
    return
  fi
  if [[ -f "${ROOT_DIR}/python/pyproject.toml" ]]; then
    echo "${ROOT_DIR}/python"
    return
  fi
  if [[ -f "${ROOT_DIR}/python/tentgent-daemon/pyproject.toml" ]]; then
    echo "${ROOT_DIR}/python/tentgent-daemon"
    return
  fi
  fail "could not find Python daemon project; set --project or TENTGENT_PYTHON_DIR"
}

resolve_python_env() {
  if [[ -n "${ENV_DIR_OVERRIDE}" ]]; then
    echo "${ENV_DIR_OVERRIDE}"
    return
  fi
  if [[ -n "${TENTGENT_PYTHON_ENV_DIR:-}" ]]; then
    echo "${TENTGENT_PYTHON_ENV_DIR}"
    return
  fi
  echo "${RUNTIME_HOME}/runtime/python-env"
}

resolve_uv_path() {
  if [[ -n "${UV_PATH_OVERRIDE}" ]]; then
    echo "${UV_PATH_OVERRIDE}"
    return
  fi
  if [[ -n "${TENTGENT_BOOTSTRAP_UV:-}" ]]; then
    echo "${TENTGENT_BOOTSTRAP_UV}"
    return
  fi
  if [[ ! -x "${SCRIPT_DIR}/bootstrap-uv.sh" ]]; then
    fail "bootstrap-uv.sh is missing; set --uv or TENTGENT_BOOTSTRAP_UV"
  fi

  local log_path
  log_path="$(mktemp)"
  if ! "${SCRIPT_DIR}/bootstrap-uv.sh" >"${log_path}"; then
    cat "${log_path}" >&2
    rm -f "${log_path}"
    fail "failed to bootstrap pinned uv"
  fi
  cat "${log_path}" >&2
  local uv_path
  uv_path="$(tail -n 1 "${log_path}")"
  rm -f "${log_path}"
  echo "${uv_path}"
}

print_plan() {
  cat <<PLAN
python project: ${PROJECT_DIR}
python env: ${ENV_DIR}
python version: ${PYTHON_VERSION}
uv cache: ${UV_CACHE_DIR}
uv path: ${UV_PATH:-"(resolved during sync)"}
entrypoint dir: ${ENV_DIR}/bin
PLAN
}

verify_project() {
  [[ -f "${PROJECT_DIR}/pyproject.toml" ]] || fail "pyproject.toml is missing: ${PROJECT_DIR}"
  [[ -d "${PROJECT_DIR}/src" ]] || fail "Python src directory is missing: ${PROJECT_DIR}/src"
}

verify_entrypoints() {
  local bin_dir="${ENV_DIR}/bin"
  local missing=()
  local name

  for name in python tentgent-chat-once tentgent-server tentgent-train-lora-run tentgent-hf-snapshot; do
    if [[ ! -x "${bin_dir}/${name}" ]]; then
      missing+=("${bin_dir}/${name}")
    fi
  done

  if [[ "${#missing[@]}" -gt 0 ]]; then
    printf 'error: missing expected Python runtime entry points:\n' >&2
    printf '  %s\n' "${missing[@]}" >&2
    exit 1
  fi
}

PROJECT_DIR_OVERRIDE=""
ENV_DIR_OVERRIDE=""
UV_PATH_OVERRIDE=""
DRY_RUN="false"
PRINT_PLAN="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --project)
      PROJECT_DIR_OVERRIDE="${2:-}"
      [[ -n "${PROJECT_DIR_OVERRIDE}" ]] || fail "--project requires a path"
      shift 2
      ;;
    --env)
      ENV_DIR_OVERRIDE="${2:-}"
      [[ -n "${ENV_DIR_OVERRIDE}" ]] || fail "--env requires a path"
      shift 2
      ;;
    --uv)
      UV_PATH_OVERRIDE="${2:-}"
      [[ -n "${UV_PATH_OVERRIDE}" ]] || fail "--uv requires a path"
      shift 2
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    --print-plan)
      PRINT_PLAN="true"
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

RUNTIME_HOME="$(default_runtime_home)"
PROJECT_DIR="$(cd "$(resolve_python_project)" && pwd)"
RAW_ENV_DIR="$(resolve_python_env)"
mkdir -p "$(dirname "${RAW_ENV_DIR}")"
ENV_DIR="$(cd "$(dirname "${RAW_ENV_DIR}")" && pwd)/$(basename "${RAW_ENV_DIR}")"
PYTHON_VERSION="${TENTGENT_BOOTSTRAP_PYTHON_VERSION:-${DEFAULT_PYTHON_VERSION}}"
UV_CACHE_DIR="${TENTGENT_BOOTSTRAP_UV_CACHE_DIR:-${RUNTIME_HOME}/runtime/bootstrap/uv-cache}"
UV_PATH=""

verify_project

if [[ "${PRINT_PLAN}" == "true" ]]; then
  print_plan
  exit 0
fi

UV_PATH="$(resolve_uv_path)"
[[ -x "${UV_PATH}" ]] || fail "pinned uv is missing or not executable: ${UV_PATH}"

print_plan
echo "==> Syncing managed Python environment"
mkdir -p "${UV_CACHE_DIR}"

SYNC_ARGS=(
  --no-config
  sync
  --project "${PROJECT_DIR}"
  --managed-python
  --python "${PYTHON_VERSION}"
  --frozen
  --no-editable
)

if [[ "${DRY_RUN}" == "true" ]]; then
  SYNC_ARGS+=(--dry-run)
fi

UV_PROJECT_ENVIRONMENT="${ENV_DIR}" \
  UV_MANAGED_PYTHON=1 \
  UV_CACHE_DIR="${UV_CACHE_DIR}" \
  "${UV_PATH}" "${SYNC_ARGS[@]}"

if [[ "${DRY_RUN}" == "true" ]]; then
  echo "==> Dry run complete"
  exit 0
fi

verify_entrypoints

echo "==> Python runtime environment ready"
echo "${ENV_DIR}"
