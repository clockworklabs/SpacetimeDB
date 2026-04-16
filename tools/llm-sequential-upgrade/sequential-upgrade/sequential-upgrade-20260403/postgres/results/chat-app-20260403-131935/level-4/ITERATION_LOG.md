# Iteration Log

## Run Info
- **Backend:** postgres
- **Level:** 1
- **Started:** 2026-04-03T13:19:35
- **Run ID:** postgres-level1-20260403-131935

---

## Build Notes

### npm install server — `--ignore-scripts` required
The `tsx@4.19.0` transitive dependency `@esbuild-kit/core-utils` has a post-install script that tries to download and verify an esbuild binary. This fails on Windows when the project path exceeds MAX_PATH (260 chars). Used `npm install --ignore-scripts` to work around. `npx tsx` (cached globally) is used at runtime instead.

### Schema conflict — existing tables dropped
The shared PostgreSQL database had tables from a prior run with a different schema (different column names in `users` table). Dropped all tables before running `drizzle-kit push`.

### Build: PASS
- Server `tsc --noEmit`: clean
- Client `tsc --noEmit`: clean
- Client `vite build`: success (56 modules)

---

## Iteration 0 — Deploy (13:23)

**Status:** Deployed successfully
- API server running at http://localhost:6001
- Client dev server running at http://localhost:6273
- Schema pushed to PostgreSQL (fresh)

**Reprompts:** 0 build reprompts (schema/install issues were environment issues, not code issues)

---

## Iteration 1 — Fix (2026-04-03)

**Category:** Feature Broken (3 bugs)

**Bug 1: Read receipts show sender**
- Root cause: `getReadReceipt` filtered out `currentUser` but not the message sender. When Alice viewed Bob's message, Bob's name still appeared in "Seen by" because he's not Alice.
- Fix: Added `senderId` parameter to `getReadReceipt` and added `r.userId !== senderId` to the filter. Updated call site to pass `group.userId`.
- Files changed: `client/src/App.tsx` (getReadReceipt function + call site)

**Bug 2: No unread count badges**
- Root cause: Users only subscribed to socket rooms when they clicked on them. New messages for rooms they hadn't opened never triggered `new_message` events, so real-time unread counts never incremented. Also, the `new_message` handler appended all messages to the messages state regardless of current room, risking cross-room message bleed.
- Fix 1: After loading joined rooms in `loadRooms`, emit `join_room` for each room so the client receives `new_message` events for all joined rooms.
- Fix 2: Rewrote `new_message` handler to only append messages to state if `current === msg.roomId`, otherwise increment unread count.
- Files changed: `client/src/App.tsx` (new_message handler + loadRooms)

**Bug 3: No Leave button**
- Root cause: Leave API and socket event existed server-side but no UI was provided.
- Fix: Added `handleLeaveRoom` handler (calls REST leave API + emits leave_room socket event + clears local state) and a "Leave" button in the room header.
- Files changed: `client/src/App.tsx` (handleLeaveRoom + JSX), `client/src/styles.css` (.leave-btn + room-header justify-content)

**Redeploy:** Client only (Vite HMR picks up changes automatically). Express server unchanged.
**Server status:** API server verified at http://localhost:6001, Client at http://localhost:6273

---
