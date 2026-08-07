#!/usr/bin/env bash
# What a generated build may not read, as a settings file the CLI enforces.
#
# This is a separate file so ../stack-bench/probe-sandbox.mjs can assert against
# the very rules run.sh hands a build. A probe holding its own copy of the list
# keeps passing after the real list drifts, which is worse than no probe: it
# reports assurance it no longer has.
#
# Usage:  source sandbox.sh; write_sandbox <app-dir> <backend>
#
# This benchmark strips the **UI contract:** blocks out of the prompt so a build
# has to infer the hooks rather than be told them. Transcripts from 2026-08-04
# show that protection defeated from the side: builds read
# stack-bench/contracts/level-01.json and level-02.json -- the same hook
# definitions, written down in the other benchmark -- plus its linter, and
# stack-bench grading output left inside their own app directory.
#
# --dangerously-skip-permissions cannot express any of this: it is
# bypassPermissions, which switches the permission system off entirely, deny
# rules included. Under --permission-mode acceptEdits the rules are enforced.
# acceptEdits withholds approval from Bash and a build cannot run npm install
# without it, so Bash is allowed wholesale; `cat` therefore still reaches a
# denied path, and the transcript audit stays the real control.
#
# The app lives at <run>/<backend>/results/chat-app-<ts>, INSIDE this tool's
# tree, so a blanket deny here would stop a build reading its own source. Name
# the grading material instead. backends/*.md stays readable: it is setup
# instruction, which builds are meant to have.

write_sandbox() {
  local app_dir="$1" backend="$2" out siblings=""
  out="$app_dir/.sandbox-settings.json"

  local b
  for b in spacetime postgres mongodb; do
    [[ "$b" == "$backend" ]] && continue
    siblings+="      \"Read(**/$b/results/**)\",
"
  done

  cat > "$out" <<SANDBOX_EOF
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
SANDBOX_EOF
  echo "$out"
}
