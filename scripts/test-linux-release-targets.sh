#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
install_script="${script_dir}/install.sh"
package_script="${script_dir}/package-local.sh"
package_version="$(sed -n 's/^version = "\(.*\)"/\1/p' "${root_dir}/Cargo.toml" | head -n 1)"

fail() {
  echo "test-linux-release-targets: $*" >&2
  exit 1
}

assert_contains() {
  local value="$1"
  local needle="$2"
  local label="$3"

  [[ "${value}" == *"${needle}"* ]] || fail "${label} did not contain: ${needle}"
}

assert_fails() {
  local output
  if output="$("$@" 2>&1)"; then
    fail "expected command to fail: $*"
  fi
  printf '%s' "${output}"
}

dist_snapshot() {
  if [[ -e "${root_dir}/dist" ]]; then
    find "${root_dir}/dist" -print | LC_ALL=C sort
  else
    printf '%s\n' "<missing>"
  fi
}

install_plan="$(bash "${install_script}" \
  --dry-run \
  --target x86_64-unknown-linux-gnu \
  --version 0.3.4 \
  --skip-python-bootstrap \
  --skip-doctor)"
assert_contains "${install_plan}" "target: x86_64-unknown-linux-gnu" "install dry-run"
assert_contains "${install_plan}" "tentgent-0.3.4-x86_64-unknown-linux-gnu.tar.gz" "install dry-run"

invalid_install_output="$(assert_fails bash "${install_script}" \
  --dry-run \
  --target aarch64-unknown-linux-gnu \
  --version 0.3.4 \
  --skip-python-bootstrap \
  --skip-doctor)"
assert_contains "${invalid_install_output}" "unsupported install target: aarch64-unknown-linux-gnu" "invalid install target"
if [[ "${invalid_install_output}" == *"archive:"* ]]; then
  fail "invalid install target printed an archive plan"
fi

windows_install_output="$(assert_fails bash "${install_script}" \
  --dry-run \
  --target x86_64-pc-windows-msvc \
  --version 0.3.4 \
  --skip-python-bootstrap \
  --skip-doctor)"
assert_contains "${windows_install_output}" "unsupported install target: x86_64-pc-windows-msvc" "windows install target"
if [[ "${windows_install_output}" == *"archive:"* ]]; then
  fail "windows install target printed an archive plan"
fi

dist_before_print_plan="$(dist_snapshot)"
package_plan="$(TENTGENT_TARGET=x86_64-unknown-linux-gnu bash "${package_script}" --print-plan)"
dist_after_print_plan="$(dist_snapshot)"
assert_contains "${package_plan}" "target: x86_64-unknown-linux-gnu" "package print-plan"
assert_contains "${package_plan}" "archive extension: tar.gz" "package print-plan"
assert_contains "${package_plan}" "binary name: tentgent" "package print-plan"
assert_contains "${package_plan}" "tentgent-${package_version}-x86_64-unknown-linux-gnu.tar.gz" "package print-plan"

if [[ "${dist_before_print_plan}" != "${dist_after_print_plan}" ]]; then
  fail "package --print-plan changed dist/"
fi

invalid_package_output="$(assert_fails env TENTGENT_TARGET=aarch64-unknown-linux-gnu bash "${package_script}" --print-plan)"
assert_contains "${invalid_package_output}" "unsupported package target: aarch64-unknown-linux-gnu" "invalid package target"

os="$(uname -s)"
arch="$(uname -m)"
if [[ "${os}:${arch}" != "Linux:x86_64" ]]; then
  package_output="$(assert_fails env TENTGENT_TARGET=x86_64-unknown-linux-gnu bash "${package_script}")"
  assert_contains "${package_output}" "x86_64-unknown-linux-gnu packaging must run on a native matching host" "native-host gate"
  if [[ "${package_output}" == *"Building Tentgent release binary"* ]]; then
    fail "native-host gate ran after build started"
  fi
fi

echo "linux release target tests passed"
