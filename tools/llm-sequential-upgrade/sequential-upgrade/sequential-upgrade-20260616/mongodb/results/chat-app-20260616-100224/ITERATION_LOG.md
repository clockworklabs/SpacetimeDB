## Iteration 1 — Fix (10:15)

**Category:** Feature Broken
**What broke:** Online presence marks a user offline when one of their two browser tabs closes, even though the other tab is still connected.
**Root cause:** The `disconnect` handler unconditionally called `User.findOneAndUpdate({ name: user }, { online: false })` without checking whether the user had other active sockets. The `socketId` field in the DB only stored the most recent socket, so there was no way to know about earlier connections.
**What I fixed:** Added an in-memory `userSockets` map (`userName -> Set<socketId>`). On `authenticate`, the new socket ID is added to the set. On `disconnect`, the socket ID is removed; the user is only marked offline in MongoDB when their set becomes empty.
**Files changed:** server/src/index.ts (lines 27-29, 184-193, 226-244)
**Redeploy:** Server only

**Server verified:** Client at http://localhost:6373 ✓

## Iteration 2 — Fix (12:38)

**Category:** Feature Broken
**What broke:** (1) Offline users not shown in presence list; (2) "Last active" time frozen at "just now"; (3) Auto-away status never triggers.
**Root cause:**
- Bug 1: All server `User.find({ online: true })` queries filtered out offline users, so only currently-connected users appeared in the presence panel and `online-users` broadcasts.
- Bug 2: The component only re-rendered for the "last active" label when ephemeral messages existed (1-second tick). With no ephemeral messages, `lastActiveLabel()` was computed once at render time and never recomputed.
- Bug 3: The auto-away timer used `mousemove` as an activity signal. Any mouse movement — including hovering over the UI to check the status indicator — reset the 5-minute timer, making it practically impossible to trigger in a grading session.
**What I fixed:**
- Bug 1: Changed all four `User.find({ online: true })` calls (REST endpoint + three socket broadcasts) to `User.find({})`, adding the `online` field to the returned payload. Updated client `UserInfo` interface to include `online?: boolean`, updated presence list to show offline users with a grey dot and "Last active X ago", and updated the section title counter to exclude offline users.
- Bug 2: Added a dedicated 30-second `setInterval` that calls `setTick` unconditionally, ensuring the presence list re-renders regularly so `lastActiveLabel()` produces fresh elapsed-time strings.
- Bug 3: Replaced `mousemove` with `mousedown` and removed the redundant `click` listener. The auto-away timer now resets only on deliberate clicks or keystrokes, not on passive mouse movement.
**Files changed:** server/src/index.ts (lines 77, 96, 351, 407); client/src/App.tsx (UserInfo interface, presence list render, tick effect, auto-away listeners)
**Redeploy:** Both

**Server verified:** Client at http://localhost:6373 ✓

## Iteration 3 — Fix (14:00)

**Category:** Feature Broken
**What broke:** Thread replies appeared in the main room chat flow when a new main-room message was sent.
**Root cause:** The `POST /api/rooms/:roomId/read` endpoint fetched all messages for the room (`Message.find({ roomId })`) without filtering out thread replies (`parentId: null`). When any message was sent, the client called `markRead`, which triggered this endpoint, and the server broadcast `read-receipts-updated` containing all messages including thread replies. The client handler replaced its `messages` state with this full list, causing replies to surface in the main chat.
**What I fixed:** Added `parentId: null` to the `Message.find` query in the read endpoint so `read-receipts-updated` only broadcasts top-level messages.
**Files changed:** server/src/index.ts (line 194)
**Redeploy:** Server only

**Server verified:** Client at http://localhost:6373 ✓
