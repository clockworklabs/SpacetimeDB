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

---

## Level 3 — Ephemeral Messages (Features 1–6)

| Feature | Score |
|---------|-------|
| 1–5 (regression) | 3/3 each |
| 6. Ephemeral Messages | 3/3 |
| **TOTAL** | **18/18** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L3 upgrade $1.28

---

## Level 4 — Message Reactions (Features 1–7)

| Feature | Score |
|---------|-------|
| 1–6 (regression) | 3/3 each |
| 7. Message Reactions | 3/3 |
| **TOTAL** | **21/21** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L4 upgrade $0.85

---

## Level 5 — Message Editing with History (Features 1–8)

| Feature | Score |
|---------|-------|
| 1–7 (regression) | 3/3 each |
| 8. Message Editing with History | 3/3 |
| **TOTAL** | **24/24** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L5 upgrade $1.22

---

## Level 6 — Real-Time Permissions (Features 1–9)

| Feature | Score |
|---------|-------|
| 1–8 (regression) | 3/3 each |
| 9. Real-Time Permissions | 3/3 |
| **TOTAL** | **27/27** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L6 upgrade $1.17

---

## Level 7 — Rich User Presence (Features 1–10)

| Feature | Score |
|---------|-------|
| 1–9 (regression) | 3/3 each |
| 10. Rich User Presence | 3/3 |
| **TOTAL** | **30/30** |

**Reprompt count:** 1 (multi-connection presence: user marked offline when one of several connections closed)
**Cost:** L7 upgrade $1.69 + fix $0.48 = $2.18

---

## Level 8 — Message Threading (Features 1–11)

| Feature | Score |
|---------|-------|
| 1–10 (regression) | 3/3 each |
| 11. Message Threading | 3/3 |
| **TOTAL** | **33/33** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L8 upgrade $1.60

---

## Level 9 — Private Rooms & DMs (Features 1–12)

| Feature | Score |
|---------|-------|
| 1–11 (regression) | 3/3 each |
| 12. Private Rooms & Direct Messages | 3/3 |
| **TOTAL** | **36/36** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L9 upgrade $1.46

---

## Level 10 — Room Activity Indicators (Features 1–13)

| Feature | Score |
|---------|-------|
| 1–12 (regression) | 3/3 each |
| 13. Room Activity Indicators | 3/3 |
| **TOTAL** | **39/39** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L10 upgrade $0.68

---

## Level 11 — Draft Sync (Features 1–14)

| Feature | Score |
|---------|-------|
| 1–13 (regression) | 3/3 each |
| 14. Draft Sync | 3/3 |
| **TOTAL** | **42/42** |

**Reprompt count:** 0 (passed on first grade)
**Cost:** L11 upgrade $1.07
