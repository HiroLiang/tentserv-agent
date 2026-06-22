#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"

fail() {
  echo "test-install-upgrade-readiness: $*" >&2
  exit 1
}

assert_contains() {
  local value="$1"
  local needle="$2"
  local label="$3"

  [[ "${value}" == *"${needle}"* ]] || fail "${label} did not contain: ${needle}"
}

run_cli() {
  TENTGENT_HOME="${temp_home}" cargo run -p tentgent-cli --quiet -- "$@" 2>&1
}

temp_home="$(mktemp -d "${TMPDIR:-/tmp}/tentgent-install-readiness.XXXXXX")"
cleanup() {
  rm -rf "${temp_home}"
}
trap cleanup EXIT

cd "${root_dir}"

version_output="$(run_cli -V)"
assert_contains "${version_output}" "tentgent " "version smoke"

bootstrap_plan="$(run_cli runtime bootstrap --print-plan --profile base)"
assert_contains "${bootstrap_plan}" "Tentgent runtime bootstrap" "bootstrap print-plan"
assert_contains "${bootstrap_plan}" "runtime profile: base" "bootstrap print-plan"
assert_contains "${bootstrap_plan}" "uv extras: none" "bootstrap print-plan"
assert_contains "${bootstrap_plan}" "print_plan: true" "bootstrap print-plan"

[[ ! -e "${temp_home}/runtime/bootstrap/uv" ]] || fail "bootstrap print-plan created uv tool cache"
[[ ! -e "${temp_home}/runtime/bootstrap/uv-cache" ]] || fail "bootstrap print-plan created uv package cache"

runtime_status="$(run_cli runtime status --profile base)"
assert_contains "${runtime_status}" "Tentgent runtime status" "runtime status"
assert_contains "${runtime_status}" "runtime_home:" "runtime status"
assert_contains "${runtime_status}" "profile_base:" "runtime status"

doctor_status=0
doctor_output="$(run_cli doctor)" || doctor_status=$?
assert_contains "${doctor_output}" "Tentgent doctor" "doctor"
assert_contains "${doctor_output}" "Result:" "doctor"
assert_contains "${doctor_output}" "Runtime footprint" "doctor"

if [[ "${doctor_status}" -ne 0 ]]; then
  assert_contains "${doctor_output}" "Details" "doctor failure diagnostics"
  assert_contains "${doctor_output}" "next:" "doctor failure diagnostics"
  assert_contains "${doctor_output}" "command:" "doctor failure diagnostics"
fi

echo "install and upgrade readiness smoke passed"
