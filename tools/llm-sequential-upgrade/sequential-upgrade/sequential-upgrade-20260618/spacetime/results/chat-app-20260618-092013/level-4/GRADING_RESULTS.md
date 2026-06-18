# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Backend:** spacetime
**Grading Method:** Manual browser interaction
**Setup:** post-improvements (skills gotchas + migration note + tsconfig) + SpacetimeDB 2.6.0 + forced 5-min cache

---

## Level 1 — Basic (Features 1–4)

| Feature | Score |
|---------|-------|
| 1. Basic Chat (name, create/join/leave rooms, send, online) | 3/3 |
| 2. Typing Indicators | 3/3 |
| 3. Read Receipts | 3/3 |
| 4. Unread Message Counts | 3/3 |
| **TOTAL** | **12/12** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L1 generate $1.05 (5-min cache)

---

## Level 2 — Scheduled Messages (Features 1–5)

| Feature | Score |
|---------|-------|
| 1–4 (regression) | 3/3 each |
| 5. Scheduled Messages | 3/3 |
| **TOTAL** | **15/15** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L2 upgrade $1.03 (1 publish attempt, vs 3 pre-migration-note)

---

## Level 3 — Ephemeral Messages (Features 1–6)

| Feature | Score |
|---------|-------|
| 1–5 (regression) | 3/3 each |
| 6. Ephemeral Messages | 3/3 |
| **TOTAL** | **18/18** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L3 upgrade $0.80 (1 publish, 0 errors)
