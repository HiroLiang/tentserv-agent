#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
metadata_script="${script_dir}/release-metadata.sh"

fail() {
  echo "test-release-metadata: $*" >&2
  exit 1
}

assert_metadata() {
  local tag="$1"
  local expected_version="$2"
  local expected_prerelease="$3"
  local expected_kind="$4"
  local output keys

  output="$(bash "${metadata_script}" "${tag}")"
  keys="$(printf '%s\n' "${output}" | cut -d= -f1 | paste -sd ',' -)"

  [[ "${keys}" == "tag,version,is_prerelease,release_kind" ]] \
    || fail "unexpected output keys for ${tag}: ${keys}"
  [[ "${output}" == *"tag=${tag}"* ]] \
    || fail "missing tag output for ${tag}"
  [[ "${output}" == *"version=${expected_version}"* ]] \
    || fail "missing version output for ${tag}"
  [[ "${output}" == *"is_prerelease=${expected_prerelease}"* ]] \
    || fail "missing prerelease output for ${tag}"
  [[ "${output}" == *"release_kind=${expected_kind}"* ]] \
    || fail "missing release kind output for ${tag}"
}

assert_invalid_tag() {
  local tag="$1"

  if bash "${metadata_script}" "${tag}" >/dev/null 2>&1; then
    fail "expected invalid tag to fail: ${tag}"
  fi
}

assert_flags() {
  local actual="$1"
  local expected="$2"
  local label="$3"

  [[ "${actual}" == "${expected}" ]] \
    || fail "unexpected ${label} flags: got [${actual}], expected [${expected}]"
}

source "${metadata_script}"

assert_metadata "v0.3.0" "0.3.0" "false" "stable"
assert_metadata "v0.3.0-alpha.1" "0.3.0-alpha.1" "true" "prerelease"
assert_metadata "v0.3.0-rc.1" "0.3.0-rc.1" "true" "prerelease"
assert_metadata "v1.2.3-beta.2" "1.2.3-beta.2" "true" "prerelease"

assert_invalid_tag "0.3.0"
assert_invalid_tag "v0.3"
assert_invalid_tag "main"
assert_invalid_tag "refs/tags/v0.3.0"

assert_flags "$(release_create_flags false)" "--latest" "stable create"
assert_flags "$(release_create_flags true)" $'--prerelease\n--latest=false' "prerelease create"
assert_flags "$(release_edit_flags false)" $'--draft=false\n--latest' "stable edit"
assert_flags "$(release_edit_flags true)" $'--draft=false\n--prerelease' "prerelease edit"

github_output_file="$(mktemp)"
trap 'rm -f "${github_output_file}"' EXIT
GITHUB_OUTPUT="${github_output_file}" bash "${metadata_script}" --github-output "v0.3.0-alpha.1"
github_output="$(cat "${github_output_file}")"
github_keys="$(printf '%s\n' "${github_output}" | cut -d= -f1 | paste -sd ',' -)"
[[ "${github_keys}" == "tag,version,is_prerelease,release_kind" ]] \
  || fail "unexpected github output keys: ${github_keys}"
[[ "${github_output}" == *"is_prerelease=true"* ]] \
  || fail "github output mode did not write prerelease metadata"

echo "release metadata tests passed"
