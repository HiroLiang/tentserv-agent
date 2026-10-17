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
dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  tentgent-0.3.3-x86_64-unknown-linux-gnu.tar.gz
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
assert_not_contains "${formula_path}" "x86_64-unknown-linux-gnu"
assert_not_contains "${formula_path}" "releases/download/v0.3.2"
assert_not_contains "${formula_path}" "revision "
assert_not_contains "${formula_path}" "?revision="

run_update() {
  bash "${update_script}" \
    --tag v0.3.3 \
    --tap-repo "${tmp_dir}/tap" \
    --checksums-file "${checksums_path}" "$@"
}

before_repeat="${tmp_dir}/formula-before-repeat.rb"
cp "${formula_path}" "${before_repeat}"
run_update >/dev/null
cmp "${formula_path}" "${before_repeat}" >/dev/null \
  || fail "repeated update changed the formula"

# Replacing assets under an unchanged release version must make brew upgrade
# observe a larger package revision, and a failed update must leave no edits.
sed 's/aaaaaaaa/eeeeeeee/g' "${checksums_path}" >"${tmp_dir}/replacement-checksums.txt"
mv "${tmp_dir}/replacement-checksums.txt" "${checksums_path}"
assert_fails run_update
assert_fails run_update --revision 0
cmp "${formula_path}" "${before_repeat}" >/dev/null \
  || fail "rejected same-version replacement modified the formula"

revision_dry_run="$(run_update --revision 1 --dry-run)"
[[ "${revision_dry_run}" == *"+  revision 1"* ]] \
  || fail "dry-run diff did not include the revision bump"
cmp "${formula_path}" "${before_repeat}" >/dev/null \
  || fail "revision dry-run modified the formula"

run_update --revision 1 >/dev/null
assert_contains "${formula_path}" "  revision 1"
assert_contains "${formula_path}" 'tentgent-0.3.3-aarch64-apple-darwin.tar.gz?revision=1"'
assert_contains "${formula_path}" 'tentgent-0.3.3-x86_64-apple-darwin.tar.gz?revision=1"'
assert_contains "${formula_path}" 'sha256 "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"'
cp "${formula_path}" "${before_repeat}"
run_update >/dev/null
run_update --revision 1 >/dev/null
cmp "${formula_path}" "${before_repeat}" >/dev/null \
  || fail "idempotent update did not preserve the existing revision"
assert_fails run_update --revision 0

write_checksums
assert_fails run_update --revision 1
cmp "${formula_path}" "${before_repeat}" >/dev/null \
  || fail "rejected checksum update modified the formula"
run_update --revision 2 >/dev/null
assert_contains "${formula_path}" "  revision 2"
assert_not_contains "${formula_path}" "  revision 1"
assert_contains "${formula_path}" 'tentgent-0.3.3-aarch64-apple-darwin.tar.gz?revision=2"'
assert_contains "${formula_path}" 'tentgent-0.3.3-x86_64-apple-darwin.tar.gz?revision=2"'
assert_not_contains "${formula_path}" "?revision=1"

# A later version starts a fresh Homebrew revision sequence.
sed 's/tentgent-0.3.3-/tentgent-0.3.4-/g' "${checksums_path}" >"${tmp_dir}/next-checksums.txt"
bash "${update_script}" \
  --tag v0.3.4 \
  --tap-repo "${tmp_dir}/tap" \
  --checksums-file "${tmp_dir}/next-checksums.txt" >/dev/null
assert_contains "${formula_path}" "releases/download/v0.3.4/tentgent-0.3.4-aarch64-apple-darwin.tar.gz"
assert_not_contains "${formula_path}" "revision "
assert_not_contains "${formula_path}" "?revision="

for invalid_revision in -1 1.5 abc 01 ''; do
  assert_fails run_update --revision "${invalid_revision}"
done
assert_fails run_update --revision

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
[[ "${dry_run_output}" == *"brew audit --formula hiroliang/tap/tentgent"* ]] \
  || fail "validation guidance must use the supported tap formula name"
[[ "${dry_run_output}" != *"brew audit --formula Formula/"* ]] \
  || fail "validation guidance must not use removed formula-file audit syntax"

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
