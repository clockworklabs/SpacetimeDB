# Chat App Grading Results

**Model:** Claude Code (Sonnet 4.6)
**Date:** 2026-04-01
**Prompt:** `05_edit_history.md` (upgraded from `04_reactions.md`)
**Backend:** spacetime
**Grading Method:** Automated browser interaction (exhaust-test)

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Prompt Level Used**   | 5 (edit_history)               |
| **Features Evaluated**  | 1-8                            |
| **Total Feature Score** | 24 / 24                        |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value  |
| ------------------------ | ------ |
| Lines of code (backend)  | 434    |
| Lines of code (frontend) | 1592 (+ 760 auto-gen bindings) |
| Number of files created  | 41     |
| External dependencies    | react, react-dom, spacetimedb, vite, @vitejs/plugin-react, typescript |
| Reprompt Count           | 0      |
| Reprompt Efficiency      | 10/10  |

### Cost Breakdown

| Phase | Cost | API Calls | Duration |
|-------|------|-----------|----------|
| Level 1 (generate) | $1.20 | 21 | ~11 min |
| Level 2 (upgrade)  | $0.79 | 30 | ~5 min  |
| Level 3 (upgrade)  | $1.54 | 56 | ~6 min  |
| Level 4 (upgrade)  | $1.14 | 41 | ~4 min  |
| Level 5 (upgrade)  | $0.43 | 16 | ~1.7 min |
| **Cumulative**      | **$5.10** | **164** | **~27.7 min** |

---

## Feature 1: Basic Chat (Score: 3 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create and join rooms (0.5)
- [x] Messages appear in real-time for all users in the room (1)
- [x] Online user list shows connected users (1)

**Browser Test Observations:** Alice and Bob registered, both in #General. Messages appear in real-time on both tabs. Online list correct.

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state broadcasts to other users in the room (1)
- [x] Typing indicator displays in the UI (1)
- [x] Typing indicator auto-expires after inactivity (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by" indicator displays under messages (1)
- [x] Read status updates in real-time when another user views the room (1)

**Browser Test Observations:** "Seen by Bob, Alice" and "Seen by Alice" displayed correctly under messages. Updates in real-time.

---

## Feature 4: Unread Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Badge clears when room is opened (1)
- [x] Count tracks per-user, per-room correctly (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 5: Scheduled Messages (Score: 3 / 3)

- [x] Users can compose a message and schedule it to send at a future time (1)
- [x] Show pending scheduled messages to the author (with option to cancel) (1)
- [x] Message appears in the room at the scheduled time (1)

**Browser Test Observations:** Verified in prior levels. Clock icon and datetime picker present in UI.

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages that auto-delete after a set duration (1)
- [x] Show a countdown or indicator that the message will disappear (1)
- [x] Message is permanently deleted from the database when time expires (1)

**Browser Test Observations:** Verified in prior level. "Normal"/"1 min"/"5 min" dropdown present in UI.

---

## Feature 7: Message Reactions (Score: 3 / 3)

- [x] Users can react to messages with emoji (1)
- [x] Show reaction counts on messages that update in real-time (1)
- [x] Users can toggle their own reactions on/off (1)
- [x] Display who reacted when hovering over reaction counts (bonus — title attribute)

**Implementation Notes:** Reaction picker (👍 ❤️ 😂 😮 😢) appears on message hover via `.reaction-picker` with `display: none` toggled to `flex`. Reactions stored in `reaction` table with identity + messageId + emoji. `reaction-btn` shows count with `reaction-btn-active` class for own reactions. Title attribute shows reactor names on hover.

**Browser Test Observations:**
1. Alice clicked 👍 on Bob's message — "👍 1" badge appeared on both tabs in real-time with `reaction-btn-active` class on Alice's tab.
2. Bob clicked ❤️ — "❤️ 1" appeared alongside "👍 1". Bob's tab showed ❤️ as active, 👍 as not active. Alice's tab showed the inverse.
3. Bob toggled ❤️ off — badge disappeared (count was 1, so entire reaction removed). Only "👍 1" remained.
4. Hover tooltip on 👍 badge showed "Alice" (title attribute).

---

## Feature 8: Message Editing with History (Score: 3 / 3)

- [x] Users can edit their own messages after sending (1)
- [x] Show "(edited)" indicator on edited messages (1)
- [x] Other users can view the edit history of a message (1)
- [x] Edits sync in real-time to all viewers (bonus)

**Implementation Notes:** ✏️ edit button (`.edit-btn`) appears on hover for own messages only. Clicking opens inline edit form with Save/Cancel buttons. `(edited)` badge (`.edited-badge`) is a clickable button that expands an "EDIT HISTORY" panel showing previous versions with timestamps. `message_edit` table stores edit history in SpacetimeDB. Real-time sync via SpacetimeDB subscriptions.

**Browser Test Observations:**
1. Alice clicked ✏️ on her message "This message will be edited soon" — inline edit form appeared with text pre-filled.
2. Changed text to "This message has been EDITED by Alice!" and clicked Save.
3. Both tabs show updated text with `(edited)` badge next to timestamp.
4. Bob clicked `(edited)` badge — "EDIT HISTORY" panel expanded showing "04:47 PM This message will be edited soon" (original text).
5. No edit button visible on Bob's tab for Alice's messages (correct authorization).

---

## Reprompt Log

| # | Iteration | Category | Issue Summary | Fixed? |
|---|-----------|----------|---------------|--------|
| - | -         | -        | No reprompts needed | N/A |

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 3 | All criteria passing |
| 2. Typing Indicators | 3 | 3 | All working |
| 3. Read Receipts | 3 | 3 | Real-time updates |
| 4. Unread Counts | 3 | 3 | Badge + clear working |
| 5. Scheduled Messages | 3 | 3 | All working |
| 6. Ephemeral Messages | 3 | 3 | Countdown + auto-delete |
| 7. Message Reactions | 3 | 3 | React, count, toggle, hover all working |
| 8. Message Editing with History | 3 | 3 | Edit, (edited) badge, history panel, real-time sync |
| **TOTAL** | **24** | **24** | |
