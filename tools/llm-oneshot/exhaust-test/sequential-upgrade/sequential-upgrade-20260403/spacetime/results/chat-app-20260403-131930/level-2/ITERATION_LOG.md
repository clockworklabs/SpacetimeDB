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
