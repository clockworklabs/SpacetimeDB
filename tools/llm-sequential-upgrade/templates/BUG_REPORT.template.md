<!--
  BUG_REPORT.md — TEMPLATE
  ========================
  Written by the GRADER (you), filed into the app directory:
    <variant>/<variant>-DATE/<backend>/results/chat-app-<ts>/BUG_REPORT.md

  The fix agent reads this verbatim (run.sh --fix keys on its existence).
  When every feature passes, DELETE this file — its absence tells the harness
  the app is done.

  Conventions (keep these for cross-backend comparability — spacetime / postgres / mongodb):
    - Write ONLY from observed browser behavior, never from source code.
    - Reference the FEATURE and user-visible behavior, not the implementation.
    - One file = all currently-open bugs, numbered `## Bug 1`, `## Bug 2`, ...
    - Pick ONE body style per bug:
        (a) Description / Expected / Actual            — state & logic bugs
        (b) Steps to reproduce / Expected / Actual     — interaction bugs
    - Optional fields: **Severity:**, **Note:**, **Fix required:** (a pointer for
      the fix agent, e.g. "check the reducer/subscription path").

  Delete this comment block in the real file.
-->

# Bug Report

## Bug 1: <one-line title of what is broken>

**Feature:** <Feature Name> (Feature N)
**Severity:** Critical — feature non-functional   <!-- optional; omit if minor -->

**Description:** <what is wrong, in behavioral terms>

**Expected:** <what should happen>
**Actual:** <what actually happens>

<!-- Optional pointer for the fix agent: -->
**Fix required:** <where to look / what to debug>


## Bug 2: <one-line title>

**Feature:** <Feature Name>

**Steps to reproduce:**
1. <step>
2. <step>
3. Expected: <expected behavior>
4. Actual: <actual behavior>

**Note:** <e.g. "Regular (non-ephemeral) messages still work correctly.">
