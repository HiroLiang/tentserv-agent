#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
metadata_script="${script_dir}/release-metadata.sh"
github_repo_url="https://github.com/HiroLiang/tentserv-agent"

fail() {
  echo "update-homebrew-formula: $*" >&2
  exit 1
}

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/update-homebrew-formula.sh --tag vX.Y.Z [OPTIONS]

Options:
  --tag <TAG>              Stable release tag to publish in the tap formula.
  --tap-repo <PATH>        Homebrew tap checkout. Defaults to `brew --repository hiroliang/tap`.
  --checksums-file <PATH>  Read release checksums from a local file instead of GitHub.
  --dry-run                Print the formula diff without writing it.
  -h, --help               Print this help.
USAGE
}

tag=""
tap_repo=""
checksums_file=""
dry_run=false

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --tag)
      [[ "$#" -ge 2 ]] || fail "--tag requires a value"
      tag="$2"
      shift 2
      ;;
    --tap-repo)
      [[ "$#" -ge 2 ]] || fail "--tap-repo requires a value"
      tap_repo="$2"
      shift 2
      ;;
    --checksums-file)
      [[ "$#" -ge 2 ]] || fail "--checksums-file requires a value"
      checksums_file="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ -n "${tag}" ]] || fail "--tag is required"
[[ -f "${metadata_script}" ]] || fail "release metadata helper not found: ${metadata_script}"

# shellcheck source=scripts/release-metadata.sh
source "${metadata_script}"
resolve_release_metadata "${tag}" || exit $?

if [[ "${RELEASE_IS_PRERELEASE}" == "true" ]]; then
  fail "prerelease tags are not supported for the stable Homebrew formula: ${RELEASE_TAG}"
fi

if [[ -z "${tap_repo}" ]]; then
  command -v brew >/dev/null 2>&1 || fail "brew is required when --tap-repo is omitted"
  tap_repo="$(brew --repository hiroliang/tap)"
fi

formula_path="${tap_repo}/Formula/tentgent.rb"
[[ -d "${tap_repo}" ]] || fail "tap checkout not found: ${tap_repo}"
[[ -f "${formula_path}" ]] || fail "formula not found: ${formula_path}"
command -v python3 >/dev/null 2>&1 || fail "python3 is required to update the formula"

tmp_checksums=""
cleanup() {
  if [[ -n "${tmp_checksums}" ]]; then
    rm -f "${tmp_checksums}"
  fi
}
trap cleanup EXIT

release_url="${github_repo_url}/releases/download/${RELEASE_TAG}"

if [[ -z "${checksums_file}" ]]; then
  command -v curl >/dev/null 2>&1 || fail "curl is required when --checksums-file is omitted"
  tmp_checksums="$(mktemp)"
  curl -fsSL "${release_url}/checksums.txt" >"${tmp_checksums}" \
    || fail "failed to download checksums.txt for ${RELEASE_TAG}"
  checksums_file="${tmp_checksums}"
fi

[[ -f "${checksums_file}" ]] || fail "checksums file not found: ${checksums_file}"

checksum_for() {
  local artifact="$1"
  local checksum

  if ! checksum="$(awk -v want="${artifact}" '
    {
      name = $2
      sub(/^\*/, "", name)
      if (name == want) {
        print $1
        found = 1
        exit
      }
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "${checksums_file}")"; then
    fail "missing checksum entry for ${artifact} in ${checksums_file}"
  fi

  if [[ ! "${checksum}" =~ ^[0-9a-fA-F]{64}$ ]]; then
    fail "invalid sha256 for ${artifact}: ${checksum}"
  fi

  printf '%s' "${checksum}"
}

arm_artifact="tentgent-${RELEASE_VERSION}-aarch64-apple-darwin.tar.gz"
intel_artifact="tentgent-${RELEASE_VERSION}-x86_64-apple-darwin.tar.gz"
arm_url="${release_url}/${arm_artifact}"
intel_url="${release_url}/${intel_artifact}"
arm_sha="$(checksum_for "${arm_artifact}")"
intel_sha="$(checksum_for "${intel_artifact}")"

FORMULA_PATH="${formula_path}" \
ARM_URL="${arm_url}" \
ARM_SHA="${arm_sha}" \
INTEL_URL="${intel_url}" \
INTEL_SHA="${intel_sha}" \
DRY_RUN="${dry_run}" \
python3 - <<'PY'
import difflib
import os
from pathlib import Path
import re
import sys

formula_path = Path(os.environ["FORMULA_PATH"])
dry_run = os.environ["DRY_RUN"] == "true"
targets = [
    ("aarch64-apple-darwin", os.environ["ARM_URL"], os.environ["ARM_SHA"]),
    ("x86_64-apple-darwin", os.environ["INTEL_URL"], os.environ["INTEL_SHA"]),
]

original = formula_path.read_text()
lines = original.splitlines(keepends=True)
updated = list(lines)

for target, url, sha in targets:
    url_pattern = re.compile(
        r'^(?P<indent>\s*)url "https://github\.com/HiroLiang/tentserv-agent/releases/download/[^"]+/tentgent-[^"]+-'
        + re.escape(target)
        + r'\.tar\.gz"\s*$'
    )
    matches = []
    for index, line in enumerate(updated):
        match = url_pattern.match(line.rstrip("\n"))
        if match:
            matches.append((index, match.group("indent")))

    if len(matches) != 1:
        print(
            f"update-homebrew-formula: expected one {target} url in {formula_path}, found {len(matches)}",
            file=sys.stderr,
        )
        sys.exit(1)

    url_index, url_indent = matches[0]
    sha_index = url_index + 1
    if sha_index >= len(updated):
        print(
            f"update-homebrew-formula: missing sha256 line after {target} url",
            file=sys.stderr,
        )
        sys.exit(1)

    sha_match = re.match(
        r'^(?P<indent>\s*)sha256 "[0-9a-fA-F]{64}"\s*$',
        updated[sha_index].rstrip("\n"),
    )
    if not sha_match:
        print(
            f"update-homebrew-formula: expected sha256 line after {target} url",
            file=sys.stderr,
        )
        sys.exit(1)

    updated[url_index] = f'{url_indent}url "{url}"\n'
    updated[sha_index] = f'{sha_match.group("indent")}sha256 "{sha}"\n'

new_text = "".join(updated)
if new_text == original:
    print(f"formula already current: {formula_path}")
    sys.exit(0)

if dry_run:
    sys.stdout.writelines(
        difflib.unified_diff(
            original.splitlines(keepends=True),
            new_text.splitlines(keepends=True),
            fromfile=str(formula_path),
            tofile=str(formula_path) + " (updated)",
        )
    )
else:
    formula_path.write_text(new_text)
    print(f"updated formula: {formula_path}")
PY

cat <<NEXT

Next validation commands:
  cd "${tap_repo}"
  brew audit --formula Formula/tentgent.rb
  brew install --formula Formula/tentgent.rb
  brew test tentgent
NEXT
