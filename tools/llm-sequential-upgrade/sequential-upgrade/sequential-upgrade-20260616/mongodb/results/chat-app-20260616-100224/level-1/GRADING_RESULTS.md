# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 1
**Grading Method:** Manual browser interaction

---

## Feature 1: Basic Chat (Score: 3 / 3)
- [x] Set a display name
- [x] Create chat rooms
- [x] Join/leave rooms
- [x] Send messages to joined rooms
- [x] Online users displayed
- [x] Basic validation (empty messages / name limits)
**Browser Test Observations:** All criteria pass after fix. Initial generate had an
online-presence bug (a user with a live session showed offline once a second tab/session
closed); fixed in iteration 1 via session ref-counting. Presence now reflects real
connection state.

## Feature 2: Typing Indicators (Score: 3 / 3)
- [x] Typing broadcast to other room members
- [x] Auto-expires after inactivity
- [x] "X is typing…" UI
**Browser Test Observations:** Works in real time.

## Feature 3: Read Receipts (Score: 3 / 3)
- [x] Tracks who has seen which messages
- [x] "Seen by X" under messages
- [x] Real-time updates
**Browser Test Observations:** Works in real time; sender excluded from own seen-by.

## Feature 4: Unread Message Counts (Score: 3 / 3)
- [x] Unread badge on room list
- [x] Tracks last-read per user per room
- [x] Real-time updates (arrive + clear)
**Browser Test Observations:** Accurate count, updates live, clears on open.

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1. Basic Chat | 3/3 | presence bug fixed in iteration 1 |
| 2. Typing Indicators | 3/3 | |
| 3. Read Receipts | 3/3 | |
| 4. Unread Counts | 3/3 | |
| **TOTAL** | **12/12** | |

**Reprompt count:** 1 (presence fix)
**Cost:** generate $0.91 + fix $0.24 = $1.15
