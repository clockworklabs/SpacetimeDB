# Bug Report

## Bug 1: Guest identity not persisted across page refresh

**Feature:** Anonymous to Registered Migration (Level 12)

**Description:** When a user joins as guest and then refreshes the page, the app returns them to the login screen. Clicking "Join as Guest" again creates a brand-new `Guest-XXXX` account with a different ID. The previous guest's messages and room memberships are orphaned (still in the DB under the old user id, but no longer accessible to the current browser session).

This violates the L12 spec line: *"Anonymous users have a temporary identity that persists for their session."*

**Root cause to investigate:** The client (`client/src/App.tsx`) does not persist `currentUser` anywhere. There is no `localStorage` or `sessionStorage` usage at all in App.tsx. On page load, `currentUser` starts as `null`, the login screen is shown, and clicking "Join as Guest" always calls `POST /api/users/anonymous` which creates a new row. The same applies to registered users — refreshing also logs them out. For guests this is particularly broken because they have no way to log back in by name.

**Steps to reproduce:**
1. Open http://localhost:6273/ in a fresh incognito window
2. Click "Join as Guest" → app assigns `Guest-ABCD`
3. Join a public room, send 2 messages
4. Press F5 to refresh the page
5. App shows login screen again (bug)
6. Click "Join as Guest" again → app assigns `Guest-WXYZ` (different ID, different name)
7. Previous messages sent as `Guest-ABCD` are still in the DB under that user id but this new session has no connection to them

**Expected:** After refresh, the previous guest session is restored. The user continues as `Guest-ABCD`, sees their messages with their name, and remains a member of any rooms they had joined.

**Actual:** Each refresh spawns a new guest identity. Previous guest data is orphaned.

**Fix guidance:**
- Persist `currentUser` (at minimum `id` and `isAnonymous`) to `localStorage` whenever it is set (after `handleLogin`, `handleJoinAsGuest`, `handleRegister`, or on the `user_identity_updated` socket event)
- On app mount, check `localStorage` for a stored user id. If present, fetch `GET /api/users/:id` to rehydrate the full user and set `currentUser`
- If the stored user no longer exists on the server (404), clear the stored id and show the login screen normally
- This fix should apply to BOTH guests and registered users — the same refresh-loses-session bug exists for registered users but is less visible because they can just re-enter their name. For guests there is no recovery path, which is why we're filing it now

**Files likely to change:**
- `client/src/App.tsx` (add localStorage get/set + mount-time rehydration)
- Possibly `server/src/index.ts` if `GET /api/users/:id` does not already exist (check first — it likely does)
