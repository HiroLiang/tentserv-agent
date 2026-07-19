#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TENTGENT_BIN="${TENTGENT_BIN:-$ROOT_DIR/target/debug/tentgent}"
DATA_ROOT="${TENTGENT_SMOKE_DATA_ROOT:-${TENTGENT_DATA_ROOT:-}}"
PORT="${TENTGENT_SMOKE_PORT:-18780}"
CHAT_MODEL_REF="${TENTGENT_SMOKE_CHAT_MODEL_REF:-d33b2cfb24fd814322fd365e21b6d9895130b02edaa5e29aaffa906eef5e09fd}"
EMBEDDING_MODEL_REF="${TENTGENT_SMOKE_EMBEDDING_MODEL_REF:-804843a8a9d1c83c9a5cee3c3e950e873d2764c9405cc38e9dd4c6a4182a389a}"
RERANK_MODEL_REF="${TENTGENT_SMOKE_RERANK_MODEL_REF:-abcf09f1cd183ab8ead21f2d69d8013057b451351bf62250da00662807de0a64}"
AUDIO_MODEL_REF="${TENTGENT_SMOKE_AUDIO_MODEL_REF:-9e9bbd1515bcc41702611912fdbdfb427714271a566c1274c876931d8b10a27d}"
VISION_MODEL_REF="${TENTGENT_SMOKE_VISION_MODEL_REF:-fb5f2f1a4b1f50295f199c98e8d33bac8bf207cb5d4af413b38bb9effd0176e3}"
AUDIO_PATH="${TENTGENT_SMOKE_AUDIO_PATH:-$ROOT_DIR/test-data/we_go_up.mp3}"
IMAGE_PATH="${TENTGENT_SMOKE_IMAGE_PATH:-$ROOT_DIR/test-data/test_image.jpg}"
CONTROL_HOME="$(mktemp -d "${TMPDIR:-/tmp}/tentgent-cluster-smoke.XXXXXX")"
CLUSTER_TOML="$CONTROL_HOME/cluster.toml"
BLOCK_TOML="$CONTROL_HOME/cluster-block.toml"
SERVER_REF=""
KEEP_HOME="${TENTGENT_SMOKE_KEEP_HOME:-false}"

if [[ ! -x "$TENTGENT_BIN" ]]; then
  echo "error: Tentgent binary not found at $TENTGENT_BIN" >&2
  echo "build it with: cargo build -p tentgent-cli" >&2
  exit 1
fi
if [[ -z "$DATA_ROOT" ]]; then
  echo "error: set TENTGENT_SMOKE_DATA_ROOT to the existing Tentgent data root" >&2
  exit 1
fi
if [[ ! -f "$AUDIO_PATH" || ! -f "$IMAGE_PATH" ]]; then
  echo "error: smoke audio or image fixture is missing" >&2
  exit 1
fi

cleanup() {
  local status="$?"
  if [[ "$status" -ne 0 ]]; then
    echo "error: cluster runtime ownership smoke failed (exit $status)" >&2
    if [[ -n "$SERVER_REF" ]]; then
      local stderr_log="$CONTROL_HOME/servers/$SERVER_REF/stderr.log"
      if [[ -f "$stderr_log" ]]; then
        echo "--- cluster server stderr ---" >&2
        tail -n 80 "$stderr_log" >&2 || true
      fi
    fi
  fi
  if [[ -n "$SERVER_REF" ]]; then
    "$TENTGENT_BIN" server stop --home "$CONTROL_HOME" "$SERVER_REF" >/dev/null 2>&1 || true
    "$TENTGENT_BIN" server rm --home "$CONTROL_HOME" "$SERVER_REF" >/dev/null 2>&1 || true
  fi
  "$TENTGENT_BIN" runtime reconcile --home "$CONTROL_HOME" --apply >/dev/null 2>&1 || true
  if [[ "$KEEP_HOME" == "true" || "$status" -ne 0 ]]; then
    echo "smoke control home retained at: $CONTROL_HOME" >&2
  else
    rm -rf "$CONTROL_HOME"
  fi
}
trap cleanup EXIT

cat >"$CLUSTER_TOML" <<EOF
schema_version = 1
cluster_ref = "runtime-ownership-smoke"
route_update_policy = "drain"

[routes.chat]
kind = "local-model"
model_ref = "$CHAT_MODEL_REF"

[routes.embedding]
kind = "local-model"
model_ref = "$EMBEDDING_MODEL_REF"

[routes.rerank]
kind = "local-model"
model_ref = "$RERANK_MODEL_REF"

[routes.audio-transcription]
kind = "local-model"
model_ref = "$AUDIO_MODEL_REF"

[routes.vision-chat]
kind = "local-model"
model_ref = "$VISION_MODEL_REF"
EOF
sed 's/route_update_policy = "drain"/route_update_policy = "block"/' \
  "$CLUSTER_TOML" >"$BLOCK_TOML"

export TENTGENT_DATA_ROOT="$DATA_ROOT"

step() {
  echo "==> $1"
}

request() {
  local label="$1"
  local expected="$2"
  shift 2
  step "$label"
  local output
  if ! output="$(curl --fail-with-body --silent --show-error --max-time 120 "$@")"; then
    echo "error: $label request failed" >&2
    echo "$output" >&2
    return 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    echo "error: $label response did not contain $expected" >&2
    echo "$output" >&2
    exit 1
  fi
  echo "$output"
}

step "validate and apply isolated cluster definition"
"$TENTGENT_BIN" cluster validate --home "$CONTROL_HOME" "$CLUSTER_TOML"
"$TENTGENT_BIN" cluster apply --home "$CONTROL_HOME" "$CLUSTER_TOML"

step "start detached cluster server"
RUN_OUTPUT="$("$TENTGENT_BIN" cluster run --home "$CONTROL_HOME" \
  runtime-ownership-smoke --port "$PORT" --idle-seconds 1 \
  --allow-unverified --detach)"
echo "$RUN_OUTPUT"
SERVER_REF="$(printf '%s\n' "$RUN_OUTPUT" | sed -nE \
  's/.*server_ref[^0-9a-f]*([0-9a-f]{64}).*/\1/p' | head -n 1)"
if [[ -z "$SERVER_REF" ]]; then
  echo "error: could not read server_ref from cluster run output" >&2
  exit 1
fi

step "wait for cluster health"
for _ in $(seq 1 60); do
  if curl --silent --max-time 2 "http://127.0.0.1:$PORT/healthz" | grep -q '"ok":true'; then
    break
  fi
  sleep 1
done
HEALTH="$(curl --fail --silent --show-error --max-time 5 "http://127.0.0.1:$PORT/healthz")"
INITIAL_HASH="$(printf '%s' "$HEALTH" | sed -nE \
  's/.*"definition_hash":"([0-9a-f]+)".*/\1/p')"
[[ -n "$INITIAL_HASH" ]] || { echo "error: health response lacks definition hash" >&2; exit 1; }

request "chat route" '"text"' "http://127.0.0.1:$PORT/v1/chat" \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"Reply with the word ready."}],"max_tokens":12,"temperature":0}'
request "embedding route" '"embedding"' "http://127.0.0.1:$PORT/v1/embeddings" \
  -H 'Content-Type: application/json' \
  -d '{"input":["cluster ownership smoke"]}'
request "rerank route" '"score"' "http://127.0.0.1:$PORT/v1/rerank" \
  -H 'Content-Type: application/json' \
  -d '{"query":"cluster safety","documents":["runtime ownership","unrelated text"],"top_n":1}'
request "audio transcription route" '"text"' \
  "http://127.0.0.1:$PORT/v1/audio/transcriptions" \
  -H 'Content-Type: application/json' \
  -d "{\"input_path\":\"$AUDIO_PATH\",\"output_path\":\"$CONTROL_HOME/transcript.txt\",\"output_format\":\"text\"}"
request "vision chat route" '"text"' "http://127.0.0.1:$PORT/v1/vision/chat" \
  -H 'Content-Type: application/json' \
  -d "{\"image_path\":\"$IMAGE_PATH\",\"prompt\":\"Describe the image in one short sentence.\",\"output_format\":\"text\",\"max_tokens\":32}"

HEALTH_AFTER_REQUESTS="$(curl --fail --silent --show-error --max-time 5 \
  "http://127.0.0.1:$PORT/healthz")"
if [[ "$HEALTH_AFTER_REQUESTS" != *'"active_request_count":0'* ]]; then
  echo "error: completed responses still hold cluster request leases" >&2
  echo "$HEALTH_AFTER_REQUESTS" >&2
  exit 1
fi

step "inspect scoped ownership"
"$TENTGENT_BIN" cluster inspect --home "$CONTROL_HOME" runtime-ownership-smoke
"$TENTGENT_BIN" server inspect --home "$CONTROL_HOME" "$SERVER_REF"

step "verify policy-only hot reload"
"$TENTGENT_BIN" cluster apply --home "$CONTROL_HOME" "$BLOCK_TOML"
RELOADED_HASH=""
for _ in $(seq 1 10); do
  RELOADED_HASH="$(curl --silent --max-time 2 "http://127.0.0.1:$PORT/healthz" | sed -nE \
    's/.*"definition_hash":"([0-9a-f]+)".*/\1/p')"
  [[ -n "$RELOADED_HASH" && "$RELOADED_HASH" != "$INITIAL_HASH" ]] && break
  sleep 1
done
if [[ -z "$RELOADED_HASH" || "$RELOADED_HASH" == "$INITIAL_HASH" ]]; then
  echo "error: cluster definition hash did not reload" >&2
  exit 1
fi

step "stop server and reconcile isolated ownership"
"$TENTGENT_BIN" server stop --home "$CONTROL_HOME" "$SERVER_REF"
RECONCILE_OUTPUT="$("$TENTGENT_BIN" runtime reconcile --home "$CONTROL_HOME")"
echo "$RECONCILE_OUTPUT"
if [[ "$RECONCILE_OUTPUT" != *"route_claims: 0"* ]]; then
  echo "error: normal cluster stop left route claims behind" >&2
  exit 1
fi
step "repair idle runtime generation records"
sleep 2
"$TENTGENT_BIN" runtime reconcile --home "$CONTROL_HOME" --apply
FINAL_RECONCILE_OUTPUT="$("$TENTGENT_BIN" runtime reconcile --home "$CONTROL_HOME")"
echo "$FINAL_RECONCILE_OUTPUT"
if [[ "$FINAL_RECONCILE_OUTPUT" != *"stale_records: 0"* ]]; then
  echo "error: runtime reconciliation left stale ownership records" >&2
  exit 1
fi
echo "cluster runtime ownership smoke passed"
