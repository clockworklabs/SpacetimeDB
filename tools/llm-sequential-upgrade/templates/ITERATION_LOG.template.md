<!--
  ITERATION_LOG.md — TEMPLATE
  ===========================
  Per-iteration fix history, kept in the app directory:
    <variant>/<variant>-DATE/<backend>/results/chat-app-<ts>/ITERATION_LOG.md

  APPEND only — never overwrite. The fix agent appends a `## Iteration N` block
  after every fix cycle; the grader may add notes. The reprompt / iteration count
  here feeds the "iterations to done" investor metric, so keep exactly one
  `## Iteration N` block per fix cycle.

  `**Category:**` is one of:
    Feature Broken | Compilation/Build | Runtime/Crash | Integration | Data/State

  `**Redeploy:**` is one of: Client only | Server only | Both
    (spacetime: `spacetime publish` then restart client;
     postgres/mongodb: restart Express server and/or client)

  Delete this comment block in the real file.
-->

# Iteration Log

## Run Info
- **Backend:** <spacetime | postgres | mongodb>
- **Level:** <N>
- **Started:** <YYYY-MM-DDThh:mm:ss>
- **Run ID:** <backend>-level<N>-<timestamp>

---

## Build Notes
<!-- Environment / build workarounds that are NOT code reprompts. -->

### <build issue title>
<what happened and how it was worked around>

### Build: PASS
- Server `tsc --noEmit`: clean
- Client `tsc --noEmit`: clean
- Client `vite build`: success

---

## Iteration 0 — Deploy (hh:mm)

**Status:** Deployed successfully
- Client dev server running at http://localhost:<vite-port>
- (postgres/mongodb) API server running at http://localhost:6001

**Reprompts:** 0 build reprompts

---

## Iteration 1 — Fix (YYYY-MM-DD)

**Category:** Feature Broken (<count> bugs)

**Bug 1: <title matching the BUG_REPORT bug>**
- Root cause: <what was actually wrong>
- Fix: <what changed>
- Files changed: `<file>` (<function / section>)

**Bug 2: <title>**
- Root cause: <...>
- Fix: <...>
- Files changed: `<file>`

**Redeploy:** Client only | Server only | Both
**Server status:** Client at http://localhost:<vite-port> ✓ <!-- (+ API at :6001 for postgres/mongodb) -->
