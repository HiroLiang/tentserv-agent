#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "macos-import-codesign-certificate: $*" >&2
  exit 1
}

require_command() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    fail "required command not found: ${name}"
  fi
}

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    fail "required environment variable is missing: ${name}"
  fi
}

write_base64_env_to_file() {
  local env_name="$1"
  local output_path="$2"

  ENV_NAME="${env_name}" OUTPUT_PATH="${output_path}" python3 - <<'PY'
import base64
import os
from pathlib import Path

name = os.environ["ENV_NAME"]
output_path = Path(os.environ["OUTPUT_PATH"])
data = base64.b64decode(os.environ[name])
output_path.write_bytes(data)
PY
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  fail "Developer ID certificate import must run on macOS"
fi

require_command python3
require_command security

require_env APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_BASE64
require_env APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_PASSWORD
require_env APPLE_KEYCHAIN_PASSWORD
require_env APPLE_TEAM_ID

tmp_dir="${RUNNER_TEMP:-/tmp}"
keychain_path="${APPLE_KEYCHAIN_PATH:-${tmp_dir}/tentgent-signing.keychain-db}"
certificate_path="${tmp_dir}/tentgent-developer-id-application.p12"

write_base64_env_to_file APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_BASE64 "${certificate_path}"
chmod 600 "${certificate_path}"

rm -f "${keychain_path}"
security create-keychain -p "${APPLE_KEYCHAIN_PASSWORD}" "${keychain_path}"
security set-keychain-settings -lut 21600 "${keychain_path}"
security unlock-keychain -p "${APPLE_KEYCHAIN_PASSWORD}" "${keychain_path}"

existing_keychains=()
while IFS= read -r keychain; do
  keychain="${keychain#\"}"
  keychain="${keychain%\"}"
  [[ -n "${keychain}" ]] && existing_keychains+=("${keychain}")
done < <(security list-keychains -d user | sed 's/^[[:space:]]*//')

security list-keychains -d user -s "${keychain_path}" "${existing_keychains[@]}"
security import "${certificate_path}" \
  -k "${keychain_path}" \
  -P "${APPLE_DEVELOPER_ID_APPLICATION_CERTIFICATE_PASSWORD}" \
  -T /usr/bin/codesign \
  -T /usr/bin/security
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "${APPLE_KEYCHAIN_PASSWORD}" \
  "${keychain_path}"

codesign_identity="${APPLE_CODESIGN_IDENTITY:-}"
if [[ -z "${codesign_identity}" ]]; then
  codesign_identity="$(
    security find-identity -v -p codesigning "${keychain_path}" \
      | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' \
      | head -n 1
  )"
fi

if [[ -z "${codesign_identity}" ]]; then
  security find-identity -v -p codesigning "${keychain_path}" >&2 || true
  fail "Developer ID Application identity was not found in temporary keychain"
fi

if ! security find-identity -v -p codesigning "${keychain_path}" | grep -F "${codesign_identity}" >/dev/null; then
  security find-identity -v -p codesigning "${keychain_path}" >&2 || true
  fail "codesign identity was not found in temporary keychain: ${codesign_identity}"
fi

if [[ -n "${GITHUB_ENV:-}" ]]; then
  {
    echo "TENTGENT_MACOS_KEYCHAIN_PATH=${keychain_path}"
    echo "TENTGENT_MACOS_CODESIGN_IDENTITY=${codesign_identity}"
    echo "TENTGENT_MACOS_CODESIGN_TEAM_ID=${APPLE_TEAM_ID}"
  } >>"${GITHUB_ENV}"
fi

echo "Developer ID signing keychain is ready: ${codesign_identity}"
