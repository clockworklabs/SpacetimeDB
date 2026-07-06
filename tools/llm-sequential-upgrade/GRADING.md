# Grading Instructions

This is the manual grading session. The app has already been generated and deployed by the automated run. Your job is to test every feature in the browser and score it.

---

## Setup — Two Independent Users

You need TWO Chrome browser profiles so each user gets completely separate identity (localStorage, cookies, WebSocket connections).

1. **Browser A (default profile):** Navigate to the app URL and register as "Alice"
   - SpacetimeDB: `http://localhost:6173`
   - PostgreSQL: `http://localhost:6273`

2. **Switch to Browser B:** Use `switch_browser` to switch to the second Chrome profile

3. **Browser B:** Navigate to the SAME URL, register as "Bob"

Use `switch_browser` to go back and forth. Both browsers connect to the same backend but have separate storage and WebSocket connections.

---

## Chrome MCP Tools

- `navigate` — go to URL
- `read_page` — accessibility tree for element discovery
- `get_page_text` — visible text
- `find` — natural language element search
- `computer` — click, type, scroll, screenshot
- `form_input` — fill form fields
- `javascript_tool` — run JS for verification
- `read_console_messages` — check for errors
- `gif_creator` — record timing-sensitive features (typing indicators, ephemeral messages)

### Adaptive Element Discovery

Every generated app has different HTML. Use this fallback chain:
1. `find("send message button")`
2. `read_page` — identify by role/text
3. `get_page_text`
4. `javascript_tool` — query DOM directly

---

## Per-Feature Testing

Read the test plan from `test-plans/feature-NN-*.md` for each feature. Test in order (1 through N).

For each feature:
1. Execute the test plan steps
2. Record pass/fail for each criterion
3. Screenshot at key verification points
4. Check `read_console_messages` for JS errors
5. Score 0–3 per the rubric below
6. **IMMEDIATELY** write the score block to `GRADING_RESULTS.md` — do not wait until the end

```markdown
## Feature N: <Name> (Score: X / 3)
- [x] <criterion> (1pt)
- [ ] <criterion> (1pt)
**Browser Test Observations:** ...
---
```

---

## Scoring Rules

- Score ONLY from observed browser behavior — never from source code
- If a criterion wasn't testable (UI didn't load, element not found), score 0
- When in doubt, score lower
- JavaScript console errors during a feature test cap that feature at 2/3
- Real-time features that only work after page refresh cap at 1/3

---

## GRADING_RESULTS.md Format

```markdown
# Chat App Grading Results

**Model:** Claude Sonnet 4.6
**Date:** <YYYY-MM-DD>
**Backend:** spacetime | postgres
**Level:** <N>
**Grading Method:** Manual browser interaction

---

## Feature 1: <Name> (Score: X / 3)
- [x] <criterion> (1pt)
...
**Browser Test Observations:** ...

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1. Basic Chat | X/3 | ... |
...
| **TOTAL** | **X/33** | |
```

**Do NOT include token counts, cost estimates, or API call counts.** Cost data is in COST_REPORT.md.

---

## Reprompt Efficiency Reference

| Reprompts | Score |
|-----------|-------|
| 0 | 10/10 |
| 1 | 9/10 |
| 2 | 8/10 |
| 3 | 7/10 |
| 4–5 | 6/10 |
| 6–7 | 5/10 |
| 8–10 | 4/10 |
| 11–15 | 2/10 |
| 16+ | 0/10 |
