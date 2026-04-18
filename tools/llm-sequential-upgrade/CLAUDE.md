# Sequential Upgrade: LLM Cost-to-Done Benchmark

You are running an automated benchmark that measures the **total cost to build a fully working chat app** — comparing SpacetimeDB vs PostgreSQL.

Your job is to **generate, build, deploy, and fix** the app. Grading happens in a separate manual session — you do NOT test in the browser.

---

## Path Convention

All file paths are **relative to the `llm-sequential-upgrade/` directory** unless stated otherwise. `../` means going up to `tools/`.

Examples:
- `backends/spacetime.md` → `llm-sequential-upgrade/backends/spacetime.md`
- `../llm-oneshot/apps/chat-app/prompts/composed/01_basic.md` → `tools/llm-oneshot/apps/chat-app/prompts/composed/01_basic.md`

---

## What You Do

Depending on the mode passed in the launch prompt:

| Mode | Task |
|------|------|
| **generate** | Create the app from scratch for the given level |
| **upgrade** | Add new features from the next level prompt to existing code |
| **fix** | Read BUG_REPORT.md, fix the listed bugs, redeploy |

**CRITICAL:** Read `backends/<backend>.md` first — it has all setup, build, and deploy instructions.

---

## Anti-Contamination

Do NOT read any files under:
- `../llm-oneshot/apps/chat-app/typescript/` (graded reference implementations)
- `../llm-oneshot/apps/chat-app/staging/`
- Any other AI-generated app code in this workspace

Only read files you created, the backend instructions, and the feature prompts.

---

## Generate / Upgrade

1. Read `backends/<backend>.md` for pre-flight checks, phases, and deploy steps
2. Read the language setup: `../llm-oneshot/apps/chat-app/prompts/language/typescript-<backend>.md`
3. Read the feature prompt: `../llm-oneshot/apps/chat-app/prompts/composed/<NN>_<name>.md`
4. Follow the phases in the backend file (generate backend → bindings → client → verify → deploy)
5. Output `DEPLOY_COMPLETE` when the dev server is confirmed running

For **upgrade**: only add the NEW features from the target level. Do not rewrite existing working features.

---

## Fix

1. Read `CLAUDE.md` in the app directory for architecture and deploy instructions
2. Read `BUG_REPORT.md` — it describes exactly what's broken
3. Read the relevant source files
4. Fix each bug, redeploy, verify the server is running
5. Append to `ITERATION_LOG.md` (see format below)
6. Output `FIX_COMPLETE`

Do NOT do browser testing — that happens in the grading session.

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

## Cost Tracking

Cost is tracked automatically via OpenTelemetry — do NOT estimate tokens or produce a COST_REPORT.md. That is generated automatically after the session ends.
