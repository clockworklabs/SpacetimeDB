# Iteration Log

## Run Info
- **Backend:** spacetime
- **Level:** 1
- **Started:** 2026-04-03T13:19:30

---

## Build Phase — Initial Generation

**Reprompt 1 (TypeScript errors):**
- **Category:** Compilation/Build
- **Issue:** `useTable` returns `ReadonlyArray<T>` but component props declared mutable arrays; unused `onError`/`onCreated`/`myName` parameters caused `noUnusedLocals`/`noUnusedParameters` errors.
- **Fix:** Changed array prop types to `ReadonlyArray<T>`, removed unused parameters from `RegisterScreen`, `CreateRoomModal`, `RoomView`, and removed unused `showToast`/`toast` state.
- **Files changed:** `client/src/App.tsx`
- **Result:** `tsc --noEmit` passes, `npm run build` succeeds.

---

## Deployment

- **Module published:** `chat-app-20260403-131930` on `http://127.0.0.1:3000`
- **Dev server:** running at `http://localhost:6173`
- **Status:** READY FOR GRADING

---

## Iteration 1 — Fix (2026-04-03)

**Category:** Feature Broken
**What broke:** Read receipts showed the message sender's own name in "Seen by" list. `getReadBy` only excluded `myHex` (current viewer) but not the message sender's identity.
**What I fixed:** Added `senderHex` parameter to `getReadBy` and filtered out the message sender from read receipts. Updated call site to pass `group.sender`.
**Files changed:** `client/src/App.tsx` (lines 404-411, 437)
**Redeploy:** Client only (HMR — dev server already running on port 6173, build verified with `tsc && vite build`)

---

## Iteration 2 — Fix (2026-04-03)

**Category:** Feature Broken (2 bugs)

**Bug 1 — No Edit button on messages:**
- `message_edit` table and `editMessage` reducer were missing from the backend entirely.
- Added `messageEdit` table to `backend/spacetimedb/src/schema.ts` with columns: `id`, `messageId`, `editedBy`, `oldText`, `newText`, `editedAt`.
- Added `editMessage` reducer to `backend/spacetimedb/src/index.ts`: validates sender owns the message, stores old text in `messageEdit`, updates `message.text`.
- Re-published backend module and regenerated client bindings.
- Updated `App.tsx`: subscribed to `SELECT * FROM message_edit`, added `useTable(tables.messageEdit)`, passed edits to `RoomView`, added inline Edit button (visible on all own non-ephemeral messages) and inline edit input with Save/Cancel.

**Bug 2 — Edit history panel not real-time:**
- The history panel derives from `messageEdits` (a `useTable` reactive array), so it automatically updates whenever new edits arrive via the subscription — no additional fix needed beyond implementing it correctly with reactive data.
- Added `(edited)` clickable tag on messages with history; clicking toggles the history panel showing all prior versions sorted by time.

**Files changed:** `backend/spacetimedb/src/schema.ts`, `backend/spacetimedb/src/index.ts`, `client/src/App.tsx`
**Redeploy:** Backend republished (`spacetime publish`), bindings regenerated, client build verified (`tsc --noEmit` + `npm run build`), dev server running on port 6173 (HMR active)

---

## Iteration 3 — Fix (2026-04-03)

**Category:** Feature Broken
**What broke:** False "kicked" notification shown immediately when joining a room. `setActiveRoomId(r.id)` was called before the `joinRoom` reducer was processed, so the kick-detection effect fired with the user not yet in `members` and incorrectly showed the notification.
**What I fixed:** Added `confirmedMemberRef` (tracks whether the user has ever been confirmed as a member of the current active room) and `prevActiveRoomIdRef` (detects room switches to reset confirmation). The kick notification is now only shown if `confirmedMemberRef.current` is true — meaning the user was previously seen as a member and is no longer. On a fresh join, `confirmedMemberRef` stays false until the server confirms membership, so no false kick is triggered.
**Files changed:** `client/src/App.tsx` (lines 21-23, 62-81)
**Redeploy:** Client only (HMR — `tsc --noEmit` passes, dev server running on port 6173)

