# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Backend:** spacetime
**Grading Method:** Manual browser interaction
**Setup:** post-fix templates + full official skills (typescript-server + typescript-client) + SpacetimeDB 2.6.0

---

## Level 1 — Basic (Features 1–4)

| Feature | Score |
|---------|-------|
| 1. Basic Chat (name, create/join/leave rooms, send messages, online users) | 3/3 |
| 2. Typing Indicators (room-scoped, auto-expire) | 3/3 |
| 3. Read Receipts (others only, real-time) | 3/3 |
| 4. Unread Message Counts (per-room badges, clear on open) | 3/3 |
| **TOTAL** | **12/12** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L1 generate $1.67

---

## Level 2 — Scheduled Messages (Features 1–5)

| Feature | Score |
|---------|-------|
| 1–4 (regression) | 3/3 each |
| 5. Scheduled Messages | 3/3 |
| **TOTAL** | **15/15** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L2 upgrade $1.04
