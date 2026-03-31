# Exhaust Test: LLM Cost-to-Done Benchmark

You are running an automated benchmark that measures the **total cost to reach a fully working chat app** — comparing SpacetimeDB vs PostgreSQL.

This is NOT a one-shot test. You will generate code, deploy, test in the browser, find bugs, fix them, redeploy, and retest — looping until all features work or the iteration limit is hit. The total cumulative cost of this loop is the metric.

---

## Path Convention

All file paths in this document are **relative to the `exhaust-test/` directory** (the directory containing this CLAUDE.md) unless stated otherwise. When the prompt says `../`, it means going up to `tools/llm-oneshot/`.

Examples:
- `test-plans/feature-01-basic-chat.md` → `exhaust-test/test-plans/feature-01-basic-chat.md`
- `../apps/chat-app/prompts/composed/01_basic.md` → `tools/llm-oneshot/apps/chat-app/prompts/composed/01_basic.md`
- `../../docs/static/ai-rules/spacetimedb.mdc` → `docs/static/ai-rules/spacetimedb.mdc` (repo root)

---

## Quick Start

When asked to run the exhaust test:

1. **Read the backend-specific instructions** from `backends/spacetime.md` or `backends/postgres.md` (as specified in the launch prompt)
2. Run pre-flight checks
3. Read the prompt files (language setup + composed feature prompt)
4. Follow the phase workflow to generate and deploy (phases vary by backend — see backend file)
5. Test every feature via Chrome MCP browser interaction
6. Fix any broken features, redeploy, retest (the loop)
7. Write `ITERATION_LOG.md` after each fix iteration (durable progress tracking)
8. Write `GRADING_RESULTS.md` at the end (cost tracking is automatic via OpenTelemetry)

**CRITICAL:** Read the backend-specific file FIRST. It contains setup, code generation, and deployment instructions specific to your backend.

---

## Configuration

These are passed to you via the launch prompt from `run.sh`:

| Parameter | Default | Description |
|-----------|---------|-------------|
| Level | 1 | Composed prompt level (01-12). Level 1 = 4 features, Level 12 = all 15 |
| Backend | spacetime | `spacetime` or `postgres` |
| App directory | (provided) | Where to write generated code and results |
| Max iterations | 10 | Max test→fix loops before stopping |

---

## Phase 0: Setup (Common)

1. **Read backend-specific instructions:** `backends/<backend>.md` — contains pre-flight checks, code generation phases, and deployment steps.

2. **Verify Chrome MCP is available** by calling `read_page`. If Chrome MCP tools are not available, STOP and report the error. Browser testing is required.
   **Note:** In headless mode (`--print`), Chrome MCP is NOT available — that's expected. Browser testing is done in a separate grading session.

3. Use the **app directory provided in the launch prompt**.

4. Read prompt files:
   - Language setup: `../apps/chat-app/prompts/language/typescript-<backend>.md`
   - Feature prompt: `../apps/chat-app/prompts/composed/<NN>_<name>.md` (based on level)

5. **CRITICAL: Anti-contamination.** Do NOT read any files under:
   - `../apps/chat-app/typescript/` (graded implementations)
   - `../apps/chat-app/staging/` (other staging implementations)
   - Any other AI-generated code in this workspace

6. Note the start time for wall-clock tracking. Token costs are tracked automatically via OpenTelemetry.

---

## Phases 1-5: Generate, Build, Deploy

**These phases are backend-specific.** Follow the instructions in `backends/spacetime.md` or `backends/postgres.md`.

---

## Phase 6: Browser Testing

This is where you interact with the running app via Chrome MCP tools to test every feature. **This phase is identical for both backends** — the test plans don't care how the backend is implemented.

### 6.1 Browser Setup

1. Navigate to `http://localhost:5173` in a Chrome tab
2. Register as "Alice" (User A)
3. Open a second tab at `http://localhost:5173`
4. Register as "Bob" (User B)

Use Chrome MCP tools:
- `navigate` — go to URL
- `read_page` — read accessibility tree for element discovery
- `get_page_text` — get visible text
- `find` — find elements by natural language description
- `computer` — click, type, scroll, screenshot
- `form_input` — fill form fields
- `tabs_create_mcp` — open new tabs
- `tabs_context_mcp` — switch between tabs
- `javascript_tool` — run JS for verification
- `read_console_messages` — check for errors
- `gif_creator` — record evidence for timing-sensitive features

### 6.2 Adaptive Element Discovery

Every generated app has different HTML structure. Use this fallback chain:
1. `find("send message button")` — natural language element search
2. `read_page` — get full accessibility tree, identify by role/text
3. `get_page_text` — search for expected text patterns
4. `javascript_tool` — query DOM directly as last resort

### 6.3 Per-Feature Testing

Read the test plan for each feature from `test-plans/feature-NN-*.md`. Each test plan specifies:
- **Preconditions** — what state must exist
- **Test steps** — exact actions and verifications
- **Pass criteria** — what constitutes a passing feature
- **Evidence** — what to screenshot or record

Test features in order (1 through N based on level). For each feature:
1. Execute the test plan steps
2. Record whether each criterion passes or fails
3. Take a screenshot at key verification points
4. Check `read_console_messages` for JavaScript errors
5. Score the feature 0-3 based on the grading rubric

### 6.4 Evidence Collection

At each feature boundary:
- Take a screenshot (`computer` with screenshot action)
- Check for console errors (`read_console_messages`)
- For timing-sensitive features (typing indicators, ephemeral messages): use `gif_creator` to record the interaction

---

## Phase 7: Test-Fix Loop

After the initial test pass, enter the fix loop:

```
LOOP (iteration 1 to max_iterations):
  1. Review test results — which features scored < 3?
  2. If all features score 3/3 → EXIT LOOP (success!)
  3. For each broken feature:
     a. Identify the bug from browser observations
     b. Read the relevant source code
     c. Fix the code (backend and/or client)
  4. Redeploy (see backend-specific file for redeploy steps)
  5. Retest all features (not just the ones you fixed — regressions happen)
  6. IMMEDIATELY write iteration to ITERATION_LOG.md (see format below)
```

Each fix in this loop counts as a **reprompt**. Track the category:
- **Compilation/Build** — code doesn't compile
- **Runtime/Crash** — app crashes
- **Feature Broken** — feature exists but doesn't work correctly
- **Integration** — frontend/backend don't communicate
- **Data/State** — data not persisting or state management issues

### ITERATION_LOG.md (Durable Progress Log)

**Write this file after EVERY iteration.** If the session crashes mid-loop, this is the only durable record of what happened. Append to it — never overwrite.

Write `ITERATION_LOG.md` in the app directory. Format:

```markdown
# Iteration Log

## Run Info
- **Backend:** spacetime|postgres
- **Level:** 1
- **Started:** 2026-03-30T14:30:00

---

## Iteration 0 — Initial Test (14:35)

**Scores:** Feature 1: 3/3, Feature 2: 1/3, Feature 3: 2/3, Feature 4: 0/3
**Total:** 6/12
**Console errors:** TypeError: Cannot read property 'map' of undefined
**Failing features:**
- Feature 2 (Typing Indicators): Typing state broadcasts but never auto-expires
- Feature 3 (Read Receipts): "Seen by" text shows but doesn't update in real-time
- Feature 4 (Unread Counts): No badge UI visible

---

## Iteration 1 — Fix (14:42)

**Category:** Feature Broken
**What broke:** Typing indicator timer never clears — `setTimeout` reference lost on re-render
**What I fixed:** Moved timer to `useRef`, added cleanup in `useEffect` return
**Files changed:** client/src/App.tsx (lines 145-160)
**Redeploy:** Client only (HMR)

**Retest scores:** Feature 1: 3/3, Feature 2: 3/3, Feature 3: 2/3, Feature 4: 0/3
**Total:** 8/12
**Still failing:**
- Feature 3: Read receipts still not real-time
- Feature 4: Still no badge UI

---

## Final Result

**Total iterations:** 3
**Final score:** 12/12
**Time elapsed:** 22 minutes
**All features passing:** Yes
```

**CRITICAL:** Write to this file after EVERY iteration, not just at the end. This is your progress checkpoint.

---

## Phase 8: Final Grading

Produce `GRADING_RESULTS.md` in the app folder. Follow this exact format:

```markdown
# Chat App Grading Results

**Model:** Claude Code (Opus 4.6)
**Date:** <YYYY-MM-DD>
**Prompt:** `<prompt_filename>`
**Backend:** spacetime|postgres
**Grading Method:** Automated browser interaction (exhaust-test)

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Prompt Level Used**   | <N> (<level name>)             |
| **Features Evaluated**  | 1-<N>                          |
| **Total Feature Score** | <score> / <max>                |

- [x/] Compiles without errors
- [x/] Runs without crashing
- [x/] First-try success

| Metric                   | Value  |
| ------------------------ | ------ |
| Lines of code (backend)  | <N>    |
| Lines of code (frontend) | <N>    |
| Number of files created  | <N>    |
| External dependencies    | <list> |
| Reprompt Count           | <N>    |
| Reprompt Efficiency      | <N>/10 |

---

## Feature N: <Name> (Score: X / 3)

- [x/ ] <criterion> (<points>)
...

**Implementation Notes:** ...
**Browser Test Observations:** ...

---

## Reprompt Log

| # | Iteration | Category | Issue Summary | Fixed? |
|---|-----------|----------|---------------|--------|
| 1 | 2         | Feature  | Typing indicator never expires | Yes |
...

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | X | ... |
...
| **TOTAL** | **<max>** | **<score>** | |
```

### Scoring Rules

- Score ONLY from observed browser behavior, never from source code
- If a criterion wasn't testable (UI didn't load, couldn't find element), score 0
- When in doubt, score lower
- JavaScript console errors during a feature test cap that feature at 2
- Real-time features that only work after refresh cap at 1

### Reprompt Efficiency Score

| Reprompts | Score |
|-----------|-------|
| 0 | 10 |
| 1 | 9 |
| 2 | 8 |
| 3 | 7 |
| 4-5 | 6 |
| 6-7 | 5 |
| 8-10 | 4 |
| 11-15 | 2 |
| 16+ | 0 |

---

## Phase 9: Cost Report (Automatic via OpenTelemetry)

**Cost tracking is handled automatically — you do NOT need to estimate tokens.**

The `run.sh` launcher enables OpenTelemetry before starting Claude Code. Every API call emits exact token counts (`input_tokens`, `output_tokens`, `cache_read_tokens`, `cost_usd`) to an OTel Collector running in Docker. After the session ends, `parse-telemetry.mjs` reads the telemetry logs and generates `COST_REPORT.md` with exact per-call breakdowns.

### What you need to do

1. **Do NOT produce a `COST_REPORT.md`** — it is generated automatically after the session.
2. **Do NOT estimate tokens** — exact counts come from OpenTelemetry instrumentation.
3. **Do** produce `GRADING_RESULTS.md` (Phase 8) — this is your responsibility.
4. **Do** produce `ITERATION_LOG.md` (Phase 7) — write after every iteration.

### How the pipeline works

```
run.sh (sets CLAUDE_CODE_ENABLE_TELEMETRY=1 + OTLP env vars)
  → Claude Code emits per-request telemetry via OTLP
  → OTel Collector (Docker) writes to telemetry/logs.jsonl
  → parse-telemetry.mjs reads logs.jsonl → generates COST_REPORT.md
```

### Prerequisites (handled by the operator, not by you)

- Docker running with `docker compose -f docker-compose.otel.yaml up -d`
- The `run.sh` script was used to launch this session (sets OTel env vars)
- After session ends, `parse-telemetry.mjs` runs automatically
