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
  --revision <NUMBER>      Homebrew revision for a same-version replacement.
                          Defaults to the existing revision for the same version,
                          or zero (no revision line) for a different version.
                          Positive revisions also refresh the download cache key.
  --dry-run                Print the formula diff without writing it.
  -h, --help               Print this help.
USAGE
}

tag=""
tap_repo=""
checksums_file=""
revision=""
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
    --revision)
      [[ "$#" -ge 2 ]] || fail "--revision requires a value"
      [[ "$2" =~ ^(0|[1-9][0-9]*)$ ]] || fail "--revision must be a nonnegative integer"
      revision="$2"
      shift 2
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
RELEASE_VERSION="${RELEASE_VERSION}" \
FORMULA_REVISION="${revision}" \
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
current_versions = set()
checksums_changed = False
url_updates = []

for target, url, sha in targets:
    url_pattern = re.compile(
        r'^(?P<indent>\s*)url "https://github\.com/HiroLiang/tentserv-agent/releases/download/[^"]+/tentgent-(?P<version>[^"]+)-'
        + re.escape(target)
        + r'\.tar\.gz(?:\?revision=[0-9]+)?"\s*$'
    )
    matches = []
    for index, line in enumerate(updated):
        match = url_pattern.match(line.rstrip("\n"))
        if match:
            matches.append((index, match.group("indent"), match.group("version")))

    if len(matches) != 1:
        print(
            f"update-homebrew-formula: expected one {target} url in {formula_path}, found {len(matches)}",
            file=sys.stderr,
        )
        sys.exit(1)

    url_index, url_indent, current_version = matches[0]
    current_versions.add(current_version)
    sha_index = url_index + 1
    if sha_index >= len(updated):
        print(
            f"update-homebrew-formula: missing sha256 line after {target} url",
            file=sys.stderr,
        )
        sys.exit(1)

    sha_match = re.match(
        r'^(?P<indent>\s*)sha256 "(?P<sha>[0-9a-fA-F]{64})"\s*$',
        updated[sha_index].rstrip("\n"),
    )
    if not sha_match:
        print(
            f"update-homebrew-formula: expected sha256 line after {target} url",
            file=sys.stderr,
        )
        sys.exit(1)

    url_updates.append((url_index, url_indent, url))
    updated[sha_index] = f'{sha_match.group("indent")}sha256 "{sha}"\n'
    checksums_changed |= sha_match.group("sha").lower() != sha.lower()

if len(current_versions) != 1:
    sys.exit("update-homebrew-formula: platform formula versions must match")

revision_lines = [
    (index, line) for index, line in enumerate(updated)
    if re.match(r"^  revision\b", line)
]
if len(revision_lines) > 1:
    sys.exit("update-homebrew-formula: expected at most one formula revision")

current_revision = 0
if revision_lines:
    revision_index, revision_line = revision_lines[0]
    revision_match = re.fullmatch(r"  revision (0|[1-9][0-9]*)\s*", revision_line)
    if not revision_match:
        sys.exit("update-homebrew-formula: invalid existing formula revision")
    current_revision = int(revision_match.group(1))

same_version = current_versions == {os.environ["RELEASE_VERSION"]}
requested_revision = os.environ["FORMULA_REVISION"]
new_revision = (
    int(requested_revision) if requested_revision
    else current_revision if same_version else 0
)
if same_version and new_revision < current_revision:
    sys.exit("update-homebrew-formula: same-version formula revision cannot decrease")
if same_version and checksums_changed and new_revision <= current_revision:
    sys.exit(
        "update-homebrew-formula: same-version checksums changed; "
        f"pass --revision greater than {current_revision} so brew upgrade detects the replacement"
    )

# Homebrew keys its archive cache by URL. A revised release must not reuse the
# old tarball merely because the version and artifact filename are unchanged.
for url_index, url_indent, url in url_updates:
    if new_revision:
        url += f"?revision={new_revision}"
    updated[url_index] = f'{url_indent}url "{url}"\n'

if revision_lines:
    if new_revision:
        updated[revision_index] = f"  revision {new_revision}\n"
    else:
        del updated[revision_index]
elif new_revision:
    license_lines = [
        index for index, line in enumerate(updated) if re.match(r"^  license\b", line)
    ]
    if len(license_lines) != 1:
        sys.exit("update-homebrew-formula: expected one license line before formula revision")
    updated.insert(license_lines[0] + 1, f"  revision {new_revision}\n")

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
  brew audit --formula hiroliang/tap/tentgent
  brew upgrade --formula hiroliang/tap/tentgent  # use install for a fresh machine
  brew test hiroliang/tap/tentgent
NEXT
