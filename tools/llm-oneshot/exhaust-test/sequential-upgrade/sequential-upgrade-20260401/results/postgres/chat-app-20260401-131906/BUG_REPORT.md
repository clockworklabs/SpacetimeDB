# Bug Report

## Feature 3: Read Receipts — Score 2/3

### Bug: Stale user names in "Seen by" display

**Severity:** Functional — read receipts show wrong user names
**Affected feature:** Read Receipts (criterion 3: real-time update)

**Description:**
When User B registers after User A is already connected, User A's tab shows "Seen by User 2" instead of "Seen by Bob". The `allUsers` list is only fetched once on login (`GET /api/users` in `useEffect` on line 172-177 of `client/src/App.tsx`). New user registrations are never pushed to already-connected clients.

The `getUserName()` helper (line 76-78) falls back to `User ${userId}` when the user ID isn't in `allUsers`.

**Root cause:**
- Server (`server/src/index.ts`): The `POST /api/users` endpoint creates users but does not broadcast a socket event to notify other clients.
- Client (`client/src/App.tsx`): No socket listener for new user registrations. The `allUsers` state is populated once via REST and never updated.

**Expected behavior:**
When a new user registers, all connected clients should receive the user's name so that "Seen by" labels, online user lists, and message author names display correctly.

**Steps to reproduce:**
1. Open Tab A, register as "Alice"
2. Open Tab B, register as "Bob"
3. Both join #General
4. Alice sends a message
5. On Alice's tab, the message shows "Seen by User 2" instead of "Seen by Bob"
6. The online user list on Alice's tab also shows "User 2" instead of "Bob"

**Files to fix:**
- `server/src/index.ts` — Broadcast `user:registered` event when a new user is created (after line 59)
- `client/src/App.tsx` — Add `socket.on('user:registered', ...)` listener to update `allUsers` state
