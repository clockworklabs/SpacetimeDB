#!/usr/bin/env bash
# Write Claude Code settings that deny direct Read tool access to benchmark
# internals. Bash is allowed, so this is not filesystem isolation.

write_read_guard() {
  local app_dir="$1" backend="$2" out siblings=""
  out="$app_dir/.read-guard-settings.json"

  local b
  for b in spacetime postgres mongodb; do
    [[ "$b" == "$backend" ]] && continue
    siblings+="      \"Read(**/$b/results/**)\",
"
  done

  cat > "$out" <<READ_GUARD_EOF
{
  "permissions": {
    "allow": ["Bash"],
    "deny": [
$siblings      "Read(**/stack-bench/**)",
      "Read(**/llm-oneshot/**)",
      "Read(**/llm-sequential-upgrade/GRADING*.md)",
      "Read(**/llm-sequential-upgrade/RUNBOOK.md)",
      "Read(**/llm-sequential-upgrade/*.sh)",
      "Read(**/llm-sequential-upgrade/*.mjs)",
      "Read(**/llm-sequential-upgrade/templates/**)",
      "Read(**/llm-sequential-upgrade/telemetry/**)",
      "Read(**/llm-sequential-upgrade/perf-benchmark/**)",
      "Read(**/inputs/**)",
      "Read(**/telemetry/**)",
      "Read(**/projects/**)",
      "Read(**/.claude/**)",
      "Read(.claude/**)",
      "Read(**/memory/**)",
      "Read(**/*.local.md)"
    ]
  }
}
READ_GUARD_EOF
  echo "$out"
}
