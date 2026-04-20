#!/usr/bin/env bash
set -euo pipefail

SUMMARY_JSON_PATH="${1:-coverage-summary.json}"

if [[ ! -f "$SUMMARY_JSON_PATH" ]]; then
  echo "coverage summary json not found: $SUMMARY_JSON_PATH" >&2
  exit 1
fi

compare_ge() {
  local actual="$1"
  local threshold="$2"
  awk -v actual="$actual" -v threshold="$threshold" 'BEGIN { exit !(actual + 0 >= threshold + 0) }'
}

require_total_metric() {
  local label="$1"
  local json_path="$2"
  local threshold="$3"
  local value
  value="$(jq -r "$json_path // 0" "$SUMMARY_JSON_PATH")"

  echo "${label}: ${value}% (threshold: ${threshold}%)"
  if ! compare_ge "$value" "$threshold"; then
    echo "coverage gate failed: ${label} is below threshold" >&2
    exit 1
  fi
}

require_file_lines_coverage() {
  local rel_path="$1"
  local threshold="$2"
  local value
  value="$(
    jq -r --arg suffix "/${rel_path}" '
      .data[0].files
      | map(select(.filename | endswith($suffix)))
      | if length == 0 then "MISSING" else (.[0].summary.lines.percent | tostring) end
    ' "$SUMMARY_JSON_PATH"
  )"

  if [[ "$value" == "MISSING" ]]; then
    echo "coverage gate failed: missing file in coverage summary: ${rel_path}" >&2
    exit 1
  fi

  echo "critical path ${rel_path}: ${value}% (threshold: ${threshold}%)"
  if ! compare_ge "$value" "$threshold"; then
    echo "coverage gate failed: ${rel_path} lines coverage is below threshold" >&2
    exit 1
  fi
}

echo "Checking total coverage thresholds"
require_total_metric "line coverage" '.data[0].totals.lines.percent' "88"
require_total_metric "function coverage" '.data[0].totals.functions.percent' "80"
require_total_metric "region coverage" '.data[0].totals.regions.percent' "88"
require_total_metric "branch coverage" '.data[0].totals.branches.percent' "67"

branch_count="$(jq -r '.data[0].totals.branches.count // 0' "$SUMMARY_JSON_PATH")"
if [[ "$branch_count" == "0" ]]; then
  echo "coverage gate failed: branch count is zero; branch instrumentation is not active" >&2
  exit 1
fi

echo "Checking critical path per-file thresholds"
require_file_lines_coverage "src/commands/add.rs" "82"
require_file_lines_coverage "src/commands/init.rs" "78"
require_file_lines_coverage "src/commands/use_.rs" "92"
require_file_lines_coverage "src/commands/status.rs" "92"
require_file_lines_coverage "src/commands/remove.rs" "92"
require_file_lines_coverage "src/auth/codex.rs" "90"
require_file_lines_coverage "src/auth/claude/mod.rs" "90"
require_file_lines_coverage "src/auth/gemini.rs" "90"

echo "Coverage quality gates passed"
