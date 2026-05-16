# Grading Workflow

How to grade generated apps and iterate on fixes.

---

## Overview

```
generate → you grade → report bugs → fix LLM fixes → you re-grade → repeat until done
   ↑                                      ↑
   token-tracked                          token-tracked
```

Code generation and fix iterations are token-tracked (the benchmark metric). Grading is manual and not tracked.

---

## Step 1: Generate

```bash
# One-shot, both backends, standard rules, level 7
./run.sh --variant one-shot --level 7 --backend spacetime --rules standard --run-index 0
./run.sh --variant one-shot --level 7 --backend postgres --rules standard --run-index 1
```

After generation, apps are running at:
- **SpacetimeDB**: `http://localhost:5173` (run-index 0)
- **PostgreSQL**: `http://localhost:5274` (run-index 1)

Port offsets for parallel runs: run-index N uses ports `5173 + N*100` (spacetime) and `5174 + N*100` (postgres).

---

## Step 2: Grade

Open each app in the browser. Test every feature at the current level.

### Level 7 features (10 features, max 30 points):

| # | Feature | What to check | Max |
|---|---------|---------------|-----|
| 1 | Basic Chat | Register with name, create room, send messages, see online users | 3 |
| 2 | Typing Indicators | "is typing" shows when other user types, auto-expires after ~5s | 3 |
| 3 | Read Receipts | "Seen by X" appears under messages after another user views them | 3 |
| 4 | Unread Counts | Numeric badge on rooms with unread messages, clears when opened | 3 |
| 5 | Scheduled Messages | Schedule button, time picker, pending list with cancel option | 3 |
| 6 | Ephemeral Messages | Duration picker, countdown indicator, message auto-deletes | 3 |
| 7 | Reactions | Emoji react button on hover, count updates, toggle on/off | 3 |
| 8 | Message Editing | Edit button on own messages, "(edited)" indicator, edit history | 3 |
| 9 | Permissions | Admin badge, kick/promote buttons, immediate effect | 3 |
| 10 | Presence | Status selector (online/away/DND/invisible), colored dots | 3 |

### Scoring

- **3** = Fully works, no issues
- **2** = Mostly works, minor issues (e.g., UI glitch but feature functional)
- **1** = Partially implemented (e.g., button exists but doesn't do anything useful)
- **0** = Missing or completely broken

### Two-user features

Features 2, 3, and parts of 1/4/9 need two users to fully test. Open two browser windows:
- Window 1: register as Alice
- Window 2: register as Bob (use incognito or a different browser profile for separate identity)

If you can't test with two users, note which features were single-user tested only.

---

## Step 3: Report Bugs

Tell Claude Code the bugs. Format:

> **spacetime bugs:** typing indicators don't show, reactions button does nothing, app has no CSS styling, scheduled messages UI is just a raw checkbox

Or more detailed:

> **postgres bugs:**
> - Feature 2: No typing indicator appears at all
> - Feature 5: Schedule button exists but clicking it does nothing
> - Feature 7: Emoji picker opens but selecting an emoji throws a console error
> - General: Messages don't auto-scroll to bottom

Claude Code will:
1. Write `BUG_REPORT.md` in the app directory
2. Run `./run.sh --fix <app-dir>` to launch the fix LLM
3. Report when the fix is done

The fix cost is token-tracked and adds to the benchmark total.

---

## Step 4: Re-grade

After the fix completes, refresh the app in the browser and re-test the features that were broken.

- If new bugs are found, report them → another fix iteration
- If all features pass, you're done with this app

---

## Step 5: Record Results

After all features pass (or you hit max iterations), the results are:

- **Cost data**: automatically in `telemetry/*/cost-summary.json` (generation + all fix iterations)
- **Grading results**: you provide the final scores

Tell Claude Code:

> **spacetime final scores:** F1=3, F2=2, F3=3, F4=3, F5=1, F6=3, F7=3, F8=2, F9=3, F10=3. Total: 26/30

Claude Code will write the GRADING_RESULTS.md and generate the comparison report.

---

## Quick Reference

| Action | Command |
|--------|---------|
| Generate (one-shot) | `./run.sh --variant one-shot --level 7 --backend spacetime --rules standard` |
| Generate (sequential L1) | `./run.sh --level 1 --backend spacetime --rules standard` |
| Upgrade to level N | `./run.sh --upgrade <app-dir> --level N --resume-session` |
| Fix bugs | `./run.sh --fix <app-dir>` |
| Parse telemetry | `node parse-telemetry.mjs <telemetry-dir> --logs-file=telemetry/logs.jsonl --extract-raw` |
| Generate report | `node generate-report.mjs <run-base-dir>` |
| Reset app state | `./reset-app.sh <app-dir>` |

---

## Feature Levels

| Level | Features | Max Score |
|-------|----------|-----------|
| 1 | 1-4 (basic chat, typing, receipts, unread) | 12 |
| 7 | 1-10 (+ scheduled, ephemeral, reactions, editing, permissions, presence) | 30 |
| 12 | 1-15 (+ threading, private rooms, activity, drafts, anonymous migration) | 45 |
| 15 | 1-18 (+ pinned, profiles, mentions/notifications) | 54 |
| 19 | 1-22 (+ bookmarks, forwarding, slow mode, polls) | 66 |
