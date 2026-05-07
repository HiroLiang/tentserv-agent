#!/usr/bin/env bash

release_tag_pattern='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$'

release_metadata_error() {
  echo "release-metadata: $*" >&2
}

validate_release_tag() {
  local tag="${1:-}"

  if [[ -z "${tag}" ]]; then
    release_metadata_error "release tag is required"
    return 2
  fi

  case "${tag}" in
    refs/*)
      release_metadata_error "release tag must be a bare tag, not a ref: ${tag}"
      return 2
      ;;
  esac

  if [[ ! "${tag}" =~ ${release_tag_pattern} ]]; then
    release_metadata_error "malformed release tag: ${tag}"
    return 2
  fi
}

resolve_release_metadata() {
  local tag="${1:-}"

  validate_release_tag "${tag}" || return $?

  RELEASE_TAG="${tag}"
  RELEASE_VERSION="${tag#v}"
  if [[ "${RELEASE_VERSION}" == *-* ]]; then
    RELEASE_IS_PRERELEASE="true"
    RELEASE_KIND="prerelease"
  else
    RELEASE_IS_PRERELEASE="false"
    RELEASE_KIND="stable"
  fi
}

emit_release_metadata() {
  local tag="${1:-}"

  resolve_release_metadata "${tag}" || return $?

  printf 'tag=%s\n' "${RELEASE_TAG}"
  printf 'version=%s\n' "${RELEASE_VERSION}"
  printf 'is_prerelease=%s\n' "${RELEASE_IS_PRERELEASE}"
  printf 'release_kind=%s\n' "${RELEASE_KIND}"
}

write_release_metadata_github_output() {
  local tag="${1:-}"
  local output_path="${2:-${GITHUB_OUTPUT:-}}"

  if [[ -z "${output_path}" ]]; then
    release_metadata_error "GITHUB_OUTPUT is not set"
    return 2
  fi

  emit_release_metadata "${tag}" >>"${output_path}"
}

release_create_flags() {
  local is_prerelease="${1:-}"

  case "${is_prerelease}" in
    true)
      printf '%s\n' "--prerelease" "--latest=false"
      ;;
    false)
      printf '%s\n' "--latest"
      ;;
    *)
      release_metadata_error "is_prerelease must be true or false: ${is_prerelease}"
      return 2
      ;;
  esac
}

release_edit_flags() {
  local is_prerelease="${1:-}"

  case "${is_prerelease}" in
    true)
      printf '%s\n' "--draft=false" "--prerelease"
      ;;
    false)
      printf '%s\n' "--draft=false" "--latest"
      ;;
    *)
      release_metadata_error "is_prerelease must be true or false: ${is_prerelease}"
      return 2
      ;;
  esac
}

release_metadata_usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/release-metadata.sh <tag>
  scripts/release-metadata.sh --github-output <tag>
USAGE
}

release_metadata_main() {
  if [[ "$#" -eq 1 ]]; then
    emit_release_metadata "$1"
    return $?
  fi

  if [[ "$#" -eq 2 && "$1" == "--github-output" ]]; then
    write_release_metadata_github_output "$2"
    return $?
  fi

  release_metadata_usage
  return 2
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  release_metadata_main "$@"
fi
