#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
bootstrap_script="${script_dir}/bootstrap-python-env.sh"
project_dir="${root_dir}/python/tentgent-model-runtime"

fail() {
  echo "test-runtime-profiles: $*" >&2
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

print_plan() {
  local profile="$1"
  local temp_home
  temp_home="$(mktemp -d /tmp/tentgent-runtime-profile.XXXXXX)"
  trap 'rm -rf "${temp_home}"' RETURN

  TENTGENT_HOME="${temp_home}" bash "${bootstrap_script}" \
    --project "${project_dir}" \
    --env "${temp_home}/python-env" \
    --profile "${profile}" \
    --print-plan

  [[ ! -e "${temp_home}/python-env" ]] || fail "--print-plan created python env"
  [[ ! -e "${temp_home}/runtime/bootstrap/uv" ]] || fail "--print-plan created uv tool cache"
  [[ ! -e "${temp_home}/runtime/bootstrap/uv-cache" ]] || fail "--print-plan created uv package cache"
}

base_plan="$(print_plan base)"
assert_contains "${base_plan}" "runtime profile: base" "base print-plan"
assert_contains "${base_plan}" "uv extras: none" "base print-plan"

local_model_plan="$(print_plan local-model)"
assert_contains "${local_model_plan}" "runtime profile: local-model" "local-model print-plan"
assert_contains "${local_model_plan}" "uv extras: local-model" "local-model print-plan"

training_plan="$(print_plan training)"
assert_contains "${training_plan}" "runtime profile: training" "training print-plan"
assert_contains "${training_plan}" "uv extras: training" "training print-plan"

full_plan="$(print_plan full)"
assert_contains "${full_plan}" "runtime profile: full" "full print-plan"
assert_contains "${full_plan}" "uv extras: local-model, training" "full print-plan"

env_profile_plan="$(
  TENTGENT_BOOTSTRAP_PROFILE=training bash "${bootstrap_script}" \
    --project "${project_dir}" \
    --print-plan
)"
assert_contains "${env_profile_plan}" "runtime profile: training" "env profile print-plan"

invalid_output="$(assert_fails bash "${bootstrap_script}" \
  --project "${project_dir}" \
  --profile train \
  --print-plan)"
assert_contains "${invalid_output}" "unsupported runtime bootstrap profile: train" "invalid profile"

echo "runtime profile tests passed"
