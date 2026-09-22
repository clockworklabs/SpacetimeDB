# Chat App: Build Instructions

Your job is to **generate, build, deploy, and fix** a fully working chat app. Verification happens in a separate session — you do NOT test in the browser.

You work only inside the app directory, which is your working directory. Everything you need is either in the launch prompt (the language setup and feature spec) or in the app directory's own `CLAUDE.md` (backend setup and deploy steps, plus phases and SDK reference at the richer rules levels). Files outside the app directory are not available and are not needed.

---

## What You Do

Depending on the mode passed in the launch prompt:

| Mode | Task |
|------|------|
| **generate** | Create the app from scratch for the given level |
| **upgrade** | Add new features from the next level prompt to existing code |
| **fix** | Read BUG_REPORT.md, fix the listed bugs, redeploy |

**CRITICAL:** Read the app directory's `CLAUDE.md` first — it has all setup, build, and deploy instructions.

---

## Shell Syntax

Windows host with both a Bash and a PowerShell tool — don't mix syntax. In the Bash tool use
POSIX: `mkdir -p` not `New-Item`, `sleep` not `Start-Sleep`, `2>/dev/null` not `2>$null`,
`VAR=x` not `$VAR=x`. PowerShell cmdlets in bash fail with "command not found".

---

## Anti-Contamination

Only read files you created, the app directory's `CLAUDE.md`, and `BUG_REPORT.md` when fixing. Do not look for reference implementations, other generated apps, or grading material anywhere on the machine.

---

## Generate / Upgrade

1. Follow the app directory's `CLAUDE.md`, including its phases in order when it has them
2. Build from the language setup and feature spec included in the launch prompt
3. Output `DEPLOY_COMPLETE` (generate) or `UPGRADE_COMPLETE` (upgrade) when the dev server is confirmed running

For **upgrade**: only add the NEW features from the target level. Do not rewrite existing working features.

---

## Fix

1. Read `CLAUDE.md` in the app directory for architecture and deploy instructions
2. Read `BUG_REPORT.md` — it describes exactly what's broken
3. Read the relevant source files
4. Fix each bug, redeploy, verify the server is running
5. Append to `ITERATION_LOG.md` (see format below)
6. Output `FIX_COMPLETE`

---

## ITERATION_LOG.md

Append to this file after every fix. Never overwrite.

```markdown
## Iteration N — Fix (HH:MM)

**Category:** Feature Broken | Compilation/Build | Runtime/Crash | Integration | Data/State
**What broke:** <short description>
**Root cause:** <what was actually wrong>
**What I fixed:** <what changed>
**Files changed:** <file (lines)>
**Redeploy:** Client only | Server only | Both

**Server verified:** Client at http://localhost:<port> ✓
```

---

## Telemetry

Do NOT estimate tokens or produce a COST_REPORT.md — that's captured automatically after the session ends.
