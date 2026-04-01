# Iteration Log

## Run Info
- **Backend:** postgres
- **Level:** 5 (edit_history)
- **Started:** 2026-04-01T00:00:00

---

## Iteration 0 — Initial State (Level 4 complete)

**Scores:** Feature 1: 3/3, Feature 2: 3/3, Feature 3: 3/3, Feature 4: 3/3, Feature 5: 3/3, Feature 6: 3/3, Feature 7: 3/3
**Total:** 21/21
**Console errors:** None
**All level 4 features passing**

---

## Level 5 Upgrade — Message Editing with History

**What was added:**
- Server already had `message_edits` table in schema, `PATCH /api/messages/:id` endpoint (stores previous content, updates `isEdited`/`editedAt`, broadcasts `message:edited` Socket.io event), and `GET /api/messages/:id/edits` endpoint
- Client already had `MessageEdit` type, `editingMessageId`/`editInput`/`historyMessageId`/`editHistory` state, `message:edited` socket handler, `startEdit`/`cancelEdit`/`handleEditSubmit`/`handleShowHistory`/`closeHistory` functions, inline edit form UI, `(edited)` indicator in message header, edit history modal
- CSS already had `.edit-form`, `.edit-input`, `.edit-btn`, `.edited-indicator`, `.modal-overlay`, `.modal`, `.modal-header`, `.modal-body`, `.edit-history-item`, `.edit-history-meta`, `.edit-history-content` styles
- Both TypeScript compilations pass with no errors
- Both servers (Express :3001, Vite :5173) running

**Files changed:** None (already implemented)
**Reprompts:** 0

---

---

## Level 6 Upgrade — Real-Time Permissions (2026-04-01)

**What was added:**

**Schema:**
- `room_admins` table: `(user_id, room_id)` composite PK, `granted_at` timestamp
- `room_bans` table: `(user_id, room_id)` composite PK, `banned_by` FK, `banned_at` timestamp
- Seeded existing room creators as admins

**Server (`src/schema.ts`):**
- Added `roomAdmins` and `roomBans` Drizzle table exports

**Server (`src/index.ts`):**
- `getRoomWithMeta`: now includes `adminIds` in the returned room object
- Room create (`POST /api/rooms`): auto-inserts creator into `room_admins`
- Join (`POST /api/rooms/:id/join`): checks `room_bans`, returns 403 if banned
- `POST /api/rooms/:id/kick`: requires admin, removes member + admin, adds ban, force-leaves socket channel (`io.in(user:X).socketsLeave(room:Y)`), emits `permission:kicked` to target user, emits `room:membership` leave to all
- `POST /api/rooms/:id/promote`: requires admin, inserts into `room_admins`, emits `permission:promoted` to room

**Client (`src/App.tsx`):**
- `Room` type: added `adminIds: number[]`
- `isAdmin` computed from `currentRoom.adminIds.includes(currentUser.id)`
- `showAdminPanel` and `kickedNotice` state
- Socket handlers: `permission:kicked` (force leave room, show notice), `permission:promoted` (update `adminIds` in rooms state)
- `handleKick(targetUserId)` and `handlePromote(targetUserId)` functions
- Chat header: shows "ADMIN" badge and "▼ Members" toggle button for admins
- Admin panel: collapsible member list with Kick + Promote buttons per non-self member, ★ Admin badge for existing admins
- Kicked notice: red banner shown when user is kicked from a room

**Files changed:** `server/src/schema.ts`, `server/src/index.ts`, `client/src/App.tsx`
**TypeScript:** Both compile clean (no errors)
**Reprompts:** 0

---

## Final Result (Level 6)

**Total iterations:** 0
**Final score:** 27/27
**All features passing:** Yes (verified by grader)

---

## Level 7 Upgrade — Rich User Presence (2026-04-01T21:40)

**New feature:** Feature 10 — Rich User Presence

### What was added

**Schema (applied via `ALTER TABLE`):**
- `status` column on `users` (text, default 'online': online | away | do-not-disturb | invisible)
- `last_active_at` column on `users` (timestamptz, default now())

**Server (`src/schema.ts`):**
- Added `status` and `lastActiveAt` fields to `users` table definition

**Server (`src/index.ts`):**
- `GET /api/users/statuses` — returns `{ userId, status, lastActiveAt }` for all users
- `PATCH /api/users/:id/status` — updates status + lastActiveAt, broadcasts `user:status` socket event
- `user:online` socket handler: updates `lastActiveAt`, broadcasts current `user:status`
- `user:activity` socket handler: updates `lastActiveAt`, auto-restores `away` → `online`
- `disconnect` handler: updates `lastActiveAt`, emits `user:status` with status `'offline'`

**Client (`src/App.tsx`):**
- `UserStatus` interface: `{ userId, status, lastActiveAt }`
- `userStatuses: Map<number, UserStatus>` state for all users
- `myStatus` state for current user's own status
- `autoAwayTimerRef`: ref for 5-minute inactivity timer
- Status selector dropdown in user badge (🟢 Online / 🟡 Away / 🔴 Do Not Disturb / ⚫ Invisible) with color-coded dot
- Users section in sidebar shows ALL registered users with colored status dots
- "Last active X ago" text shown for offline users
- Auto-away: 5 min inactivity → `PATCH /api/users/:id/status` with `away`
- Activity events (mousemove, keydown, mousedown, touchstart) emit `user:activity` to server + reset timer

**Files changed:** `server/src/schema.ts`, `server/src/index.ts`, `client/src/App.tsx`
**TypeScript:** Both compile clean
**Reprompts:** 0
