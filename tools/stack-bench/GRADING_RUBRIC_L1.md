# L1 Manual Grading Rubric — spot-check against the auto-grader

Grade each feature **0–3** from what you see in the browser. Never from source code.
Four features, 12 points total, per backend.

Windows are open for both backends. Each pair is Alice (normal window) + Bob (incognito
window) — separate storage, so they are genuinely different users.

| Backend | URL |
|---|---|
| SpacetimeDB | http://localhost:6173 |
| PostgreSQL | http://localhost:6273 |

Both databases were reset immediately before these windows opened. If you re-grade later,
reset first — a dirty database silently lowers scores.

---

## Scoring

| Score | Meaning |
|---|---|
| 3 | Fully working as specified |
| 2 | Mostly working; minor bugs or missing edge cases |
| 1 | Partial; major issues |
| 0 | Missing, broken, or untestable |

**Hard caps** (apply after scoring):
- JS console errors during a feature → cap that feature at **2**
- Feature only works after a page refresh → cap at **1**
- Couldn't test it at all → **0**

When in doubt, score lower.

---

## Feature 1 — Basic Chat (3 pts)

1 pt each:
- [ ] A message you send appears in your own message list
- [ ] Bob sees Alice's message **live, without refreshing**, and vice versa
- [ ] Online-users list shows both names; "Leave" exits the room

## Feature 2 — Typing Indicators (3 pts)

1 pt each:
- [ ] Bob types → Alice sees a "typing" indicator within a few seconds
- [ ] Indicator disappears within ~6s after Bob stops typing
- [ ] Typing is **scoped to the room** — someone in a different room sees nothing

Note: both Express apps render an always-present empty container for this. Judge the
**visible text**, not whether an element exists.

## Feature 3 — Read Receipts (3 pts)

1 pt each:
- [ ] After Bob opens the room, Alice sees a "Seen by" indicator on her message
- [ ] The receipt names Bob
- [ ] Alice is **not** listed as a reader of her own message

## Feature 4 — Unread Message Counts (3 pts)

1 pt each:
- [ ] Bob sits in room B; Alice posts in room A → a badge appears on room A for Bob
- [ ] Badge shows the correct count (send 2 messages → shows 2)
- [ ] Badge clears when Bob opens room A

---

## Invariants (separate axis, 6 pts)

Cross-cutting properties the feature spec never states — this is where the subtle
differences live, and a feature-only score hides them. Check these by hand too:

- [ ] **Same display name ≠ same account.** Register the same name in both windows. They must
  stay distinct users (one should see the other typing). On Postgres/Mongo they collapse into
  one account, so typing your own name at login *is* logging in as that person.
- [ ] **No message inheritance.** A same-named second user must not post as, or read as, the first.
- [ ] **Messages stay in their room.** A message in room A never appears in room B.
- [ ] **Your own message doesn't mark itself unread** for you.
- [ ] **A room renders exactly once** in the sidebar (no duplicate from optimistic insert + echo).
- [ ] **Reload keeps you logged in** as the same identity, with your rooms.

| Invariant group | SpacetimeDB (auto) | yours | PostgreSQL (auto) | yours |
|---|---|---|---|---|
| Identity Integrity | 2/2 | | **0/2** | |
| Data Isolation | 3/3 | | 3/3 | |
| Session Persistence | 1/1 | | **0/1** | |
| **Invariants total** | **6/6** | | **3/6** | |

## What the auto-grader said

Fill in your own column, then compare. Disagreements are the interesting part — each one
is either a grader bug to fix or a rubric ambiguity to tighten.

| Feature | SpacetimeDB (auto) | SpacetimeDB (yours) | PostgreSQL (auto) | PostgreSQL (yours) |
|---|---|---|---|---|
| 1. Basic Chat | 3 | | 3 | |
| 2. Typing Indicators | 3 | | 3 | |
| 3. Read Receipts | 3 | | 3 | |
| 4. Unread Counts | 3 | | **0** | |
| **Total** | **12** | | **9** | |

The one claim to check hardest: PostgreSQL scores **0** on unread counts — the grader says
no badge ever appears for a room you are not currently viewing. SpacetimeDB scores 3 on the
same scenario. If you can make a Postgres badge appear by hand, the grader is wrong and I
need to know.

Suggested order: grade Feature 4 on both backends first (it is the only disagreement),
then work through 1–3.
