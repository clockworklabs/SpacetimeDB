# Chat App Grading Results

**Model:** Claude Code (Sonnet 4.6)
**Date:** 2026-03-31
**Prompt:** `apps/chat-app/prompts/composed/01_basic.md`
**Backend:** postgres
**Grading Method:** Automated browser interaction (exhaust-test)

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Prompt Level Used**   | 1 (Basic Chat Features)        |
| **Features Evaluated**  | 1-4                            |
| **Total Feature Score** | 12 / 12                        |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value  |
| ------------------------ | ------ |
| Lines of code (backend)  | 429    |
| Lines of code (frontend) | 1045   |
| Number of files created  | 8      |
| External dependencies    | express, drizzle-orm, pg, socket.io, cors, dotenv, tsx, react, socket.io-client, vite |
| Reprompt Count           | 0      |
| Reprompt Efficiency      | 10/10  |

---

## Feature 1: Basic Chat Features (Score: 3 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [x] Users can join rooms (0.5)
- [x] Online users are displayed (0.5)
- [x] Users can send messages to joined rooms (0.5)
- [x] Basic validation exists — send button disabled for empty messages (0.5)

**Implementation Notes:** Registration screen with localStorage persistence. Rooms sidebar with create form. Online users panel on right. Socket.io real-time message delivery.
**Browser Test Observations:** Alice registered, "General" room created and joined. Bob registered in second tab, joined General. Messages sent bidirectionally appeared immediately in both tabs. Empty message send button disabled.

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows appropriate typing message for each user (1)

**Implementation Notes:** Server-side timeout expires typing state after ~5 seconds. Client emits `typing` socket event on input change. Server broadcasts to room and schedules `typing_stop`.
**Browser Test Observations:** Bob typing → "Bob is typing..." appeared immediately in Alice's tab. After 6 seconds with no typing: indicator disappeared. Alice typing → "Alice is typing..." appeared in Bob's tab.

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" displays under messages (1)
- [x] Read status updates in real-time (1)

**Implementation Notes:** `last_read` table tracks per-user, per-room last message ID. Socket.io `read_update` event broadcasts to room when a user reads. Client updates `seenBy` array on messages in real-time.
**Browser Test Observations:** "Hello from Alice!" showed "Seen by Bob" immediately when Bob joined and viewed the room. "Can you see this?" showed "Seen by Bob" in real-time in Alice's tab without any page refresh.

---

## Feature 4: Unread Message Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Count tracks last-read position per user per room (1)
- [x] Counts update in real-time (1)

**Implementation Notes:** Unread count computed server-side from messages after last_read position. Socket.io `unread_update` event pushes count changes. Client increments locally when message arrives for non-active room.
**Browser Test Observations:** Alice in Random room; Bob sent 3 messages to General → badge "3" appeared on General in Alice's sidebar in real-time. Alice clicked General (badge cleared). Alice returned to Random; Bob sent 1 more → badge "1" appeared immediately.

---

## Reprompt Log

| # | Iteration | Category | Issue Summary | Fixed? |
|---|-----------|----------|---------------|--------|
| — | — | — | No reprompts needed | — |

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 3 | All criteria met |
| 2. Typing Indicators | 3 | 3 | Broadcast, auto-expiry, and per-user display all working |
| 3. Read Receipts | 3 | 3 | Real-time seen-by updates |
| 4. Unread Counts | 3 | 3 | Real-time badge updates with per-user tracking |
| **TOTAL** | **12** | **12** | Perfect score, first try |
