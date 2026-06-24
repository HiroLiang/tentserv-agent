#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"

fail() {
  echo "test-release-readiness: $*" >&2
  exit 1
}

run() {
  echo "==> $*"
  "$@"
}

assert_contains() {
  local path="$1"
  local needle="$2"

  if ! rg -F -q -- "${needle}" "${path}"; then
    fail "expected ${path} to contain: ${needle}"
  fi
}

assert_not_contains() {
  local path="$1"
  local needle="$2"

  if rg -F -q -- "${needle}" "${path}"; then
    fail "expected ${path} not to contain: ${needle}"
  fi
}

assert_current_docs_use_bash_installer() {
  local output

  if output="$(rg -n 'install\.sh.*\|[[:space:]]*sh' \
    "${root_dir}/README.md" \
    "${root_dir}/docs/user/install.md" 2>/dev/null)"; then
    printf '%s\n' "${output}" >&2
    fail "current install docs must pipe install.sh to bash, not sh"
  fi
}

assert_release_workflow_patches_installers() {
  local workflow="${root_dir}/.github/workflows/release.yml"

  assert_contains "${workflow}" 'DEFAULT_BASE_URL="https://github.com/${{ github.repository }}/releases/download/${{ steps.release.outputs.tag }}"'
  assert_contains "${workflow}" 'BASE_URL="${TENTGENT_INSTALL_BASE_URL:-${DEFAULT_BASE_URL}}"'
  assert_contains "${workflow}" '$DefaultBaseUrl = "https://github.com/${{ github.repository }}/releases/download/${{ steps.release.outputs.tag }}"'
  assert_contains "${workflow}" '$BaseUrl = if ($env:TENTGENT_INSTALL_BASE_URL) { $env:TENTGENT_INSTALL_BASE_URL } else { $DefaultBaseUrl }'
}

assert_macos_release_signing_avoids_restricted_keychain_entitlements() {
  assert_contains "${root_dir}/scripts/package-local.sh" '--options runtime'
  assert_contains "${root_dir}/scripts/package-local.sh" '--identifier "${MACOS_SIGNING_IDENTIFIER}"'
  assert_contains "${root_dir}/scripts/macos-notarize-package.sh" 'TeamIdentifier=${APPLE_TEAM_ID}'
  assert_not_contains "${root_dir}/scripts/package-local.sh" 'keychain-access-groups'
  assert_not_contains "${root_dir}/scripts/package-local.sh" '--entitlements'
  assert_not_contains "${root_dir}/scripts/macos-notarize-package.sh" 'keychain-access-groups'
}

run bash -n "${script_dir}/install.sh"
run bash -n "${script_dir}/bootstrap-uv.sh"
run bash -n "${script_dir}/bootstrap-python-env.sh"
run bash -n "${script_dir}/package-local.sh"
run bash -n "${script_dir}/release-metadata.sh"
run bash -n "${script_dir}/test-release-metadata.sh"
run bash -n "${script_dir}/test-install-upgrade-readiness.sh"
run bash -n "${script_dir}/test-package-python-layout.sh"
run bash -n "${script_dir}/test-update-homebrew-formula.sh"
run bash -n "${script_dir}/update-homebrew-formula.sh"
run bash -n "${script_dir}/test-linux-release-targets.sh"
run bash -n "${script_dir}/macos-import-codesign-certificate.sh"
run bash -n "${script_dir}/macos-notarize-package.sh"

run bash "${script_dir}/test-release-metadata.sh"
run bash "${script_dir}/test-install-upgrade-readiness.sh"
run bash "${script_dir}/test-update-homebrew-formula.sh"
run bash "${script_dir}/test-linux-release-targets.sh"
run bash "${script_dir}/test-package-python-layout.sh"

run bash "${script_dir}/install.sh" \
  --dry-run \
  --target aarch64-apple-darwin \
  --version 0.0.0 \
  --skip-python-bootstrap \
  --skip-doctor

run bash "${script_dir}/install.sh" \
  --dry-run \
  --target x86_64-apple-darwin \
  --version 0.0.0 \
  --skip-python-bootstrap \
  --skip-doctor

run bash "${script_dir}/install.sh" \
  --dry-run \
  --target x86_64-unknown-linux-gnu \
  --version 0.0.0 \
  --skip-python-bootstrap \
  --skip-doctor

if command -v pwsh >/dev/null 2>&1; then
  run pwsh -NoProfile -ExecutionPolicy Bypass \
    -File "${script_dir}/install.ps1" \
    -DryRun \
    -Version "0.0.0" \
    -Target "x86_64-pc-windows-msvc" \
    -SkipPythonBootstrap \
    -SkipDoctor
else
  echo "==> Skipping PowerShell installer dry-run because pwsh is not installed"
fi

echo "==> Checking release workflow installer patching"
assert_release_workflow_patches_installers

echo "==> Checking macOS release signing avoids restricted Keychain entitlements"
assert_macos_release_signing_avoids_restricted_keychain_entitlements

echo "==> Checking current install docs use bash for install.sh"
assert_current_docs_use_bash_installer

echo "release readiness checks passed"
