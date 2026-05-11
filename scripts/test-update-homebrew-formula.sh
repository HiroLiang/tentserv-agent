#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
update_script="${script_dir}/update-homebrew-formula.sh"

fail() {
  echo "test-update-homebrew-formula: $*" >&2
  exit 1
}

assert_contains() {
  local path="$1"
  local needle="$2"

  if ! rg -F -q "${needle}" "${path}"; then
    fail "expected ${path} to contain: ${needle}"
  fi
}

assert_not_contains() {
  local path="$1"
  local needle="$2"

  if rg -F -q "${needle}" "${path}"; then
    fail "expected ${path} not to contain: ${needle}"
  fi
}

assert_fails() {
  if "$@" >/dev/null 2>&1; then
    fail "expected command to fail: $*"
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

formula_path="${tmp_dir}/tap/Formula/tentgent.rb"
checksums_path="${tmp_dir}/checksums.txt"
mkdir -p "$(dirname "${formula_path}")"

write_formula() {
  cat >"${formula_path}" <<'RUBY'
class Tentgent < Formula
  desc "Local AI runtime, dataset, server, daemon, and TUI toolkit"
  homepage "https://github.com/HiroLiang/tentserv-agent"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.2/tentgent-0.3.2-aarch64-apple-darwin.tar.gz"
      sha256 "1111111111111111111111111111111111111111111111111111111111111111"
    else
      url "https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.2/tentgent-0.3.2-x86_64-apple-darwin.tar.gz"
      sha256 "2222222222222222222222222222222222222222222222222222222222222222"
    end
  end

  def install
    bin.install "bin/tentgent"
  end
end
RUBY
}

write_checksums() {
  cat >"${checksums_path}" <<'SHA'
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  tentgent-0.3.3-aarch64-apple-darwin.tar.gz
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  tentgent-0.3.3-x86_64-apple-darwin.tar.gz
cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc *tentgent-0.3.3-x86_64-pc-windows-msvc.zip
SHA
}

write_formula
write_checksums

bash "${update_script}" \
  --tag v0.3.3 \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${checksums_path}" >/dev/null

assert_contains "${formula_path}" "releases/download/v0.3.3/tentgent-0.3.3-aarch64-apple-darwin.tar.gz"
assert_contains "${formula_path}" "releases/download/v0.3.3/tentgent-0.3.3-x86_64-apple-darwin.tar.gz"
assert_contains "${formula_path}" 'sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"'
assert_contains "${formula_path}" 'sha256 "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"'
assert_not_contains "${formula_path}" "releases/download/v0.3.2"

write_formula
before_dry_run="${tmp_dir}/formula-before-dry-run.rb"
cp "${formula_path}" "${before_dry_run}"
dry_run_output="$(bash "${update_script}" \
  --tag v0.3.3 \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${checksums_path}" \
  --dry-run)"

cmp "${formula_path}" "${before_dry_run}" >/dev/null \
  || fail "dry-run modified the formula"
[[ "${dry_run_output}" == *"+      url \"https://github.com/HiroLiang/tentserv-agent/releases/download/v0.3.3/tentgent-0.3.3-aarch64-apple-darwin.tar.gz\""* ]] \
  || fail "dry-run diff did not include updated ARM URL"
[[ "${dry_run_output}" == *"+      sha256 \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\""* ]] \
  || fail "dry-run diff did not include updated Intel sha"

missing_arm_checksums="${tmp_dir}/missing-arm-checksums.txt"
cat >"${missing_arm_checksums}" <<'SHA'
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  tentgent-0.3.3-x86_64-apple-darwin.tar.gz
SHA
assert_fails bash "${update_script}" \
  --tag v0.3.3 \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${missing_arm_checksums}"

missing_intel_checksums="${tmp_dir}/missing-intel-checksums.txt"
cat >"${missing_intel_checksums}" <<'SHA'
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  tentgent-0.3.3-aarch64-apple-darwin.tar.gz
SHA
assert_fails bash "${update_script}" \
  --tag v0.3.3 \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${missing_intel_checksums}"

assert_fails bash "${update_script}" \
  --tag main \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${checksums_path}"

assert_fails bash "${update_script}" \
  --tag v0.3.3-alpha.1 \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${checksums_path}"

echo "homebrew formula update tests passed"
