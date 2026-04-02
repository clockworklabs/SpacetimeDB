# Iteration Log

## Run Info
- **Backend:** spacetime
- **Level:** 7 (07_presence.md)
- **Started:** 2026-04-02T16:21:35
- **Features:** 1-10 (Basic Chat, Typing Indicators, Read Receipts, Unread Counts, Scheduled Messages, Ephemeral Messages, Message Reactions, Message Editing, Real-Time Permissions, Rich User Presence)

---

## Iteration 0 — Initial Deploy (Build Reprompts)

**Status:** DEPLOY COMPLETE — dev server running at http://localhost:5173

**Backend:**
- Module published: `chat-app-20260402-162135`
- Identity: `c20058854d59d98be18ade61d6e1fbecb262ed555051396ccea7c467f6129b10`
- Tables: user, room, room_member, message, message_edit, message_reaction, read_receipt, room_last_read, typing_indicator, scheduled_message + 3 scheduled cleanup tables
- Reducers: register, update_status, update_activity, create_room, join_room, leave_room, kick_user, promote_user, unban_user, send_message, send_ephemeral_message, edit_message, delete_message, set_typing, clear_typing, mark_message_read, mark_room_read, toggle_reaction, schedule_message, cancel_scheduled_message, set_away_timer

**Build issues fixed (reprompts):**

### Reprompt 1 — Backend: Wrong schema() and reducer() API
- **Category:** Compilation/Build
- **Issue:** Used `schema({ user, room })` (object form) — SDK uses spread args `schema(user, room, ...)`
- **Issue:** Used `spacetimedb.reducer({ name: t.string() }, fn)` — SDK requires name string first: `spacetimedb.reducer('name', { ... }, fn)`
- **Issue:** `scheduled: (): any => reducerRef` — must be string: `scheduled: 'reducer_name_string'`
- **Issue:** Index `columns: ['room_id']` (snake_case) — must be camelCase: `columns: ['roomId']`
- **Issue:** Index `accessor:` → `name:` field
- **Fixed:** All backend TypeScript errors resolved

### Reprompt 2 — Backend: Option types and Timestamp construction
- **Category:** Compilation/Build
- **Issue:** `null` not assignable to `Timestamp | undefined` — option fields use `undefined` not `null`
- **Issue:** `{ microsSinceUnixEpoch: N }` is not valid Timestamp — must use `new Timestamp(N)`
- **Fixed:** Third publish attempt succeeded

### Reprompt 3 — Client: SDK version mismatch
- **Category:** Integration
- **Issue:** `package.json` had `spacetimedb: ^1.1.1` but CLI generated code for SDK 2.x
- **Fixed:** Updated to `spacetimedb: ^2.0.4`, installed SDK 2.1.0

### Reprompt 4 — Client: Wrong table accessors and API methods
- **Category:** Compilation/Build
- **Issue:** `tables.roomMember` → must be `tables.room_member` (snake_case keys from generated schema)
- **Issue:** `tables.messageEdit` → `tables.message_edit`, etc. for all multi-word table names
- **Issue:** `withModuleName()` → `withDatabaseName()` in DbConnectionBuilder
- **Issue:** `MessageItemProps` typed with `User[]` but `useTable` returns `readonly User[]`
- **Issue:** `.sort()` called on `readonly` array — fixed with `[...edits].sort(...)`
- **Fixed:** All TypeScript errors resolved, build succeeded

**Final build:** ✅ `vite build` successful (289.61 kB JS, 11.59 kB CSS)
**Dev server:** ✅ Running at http://localhost:5173 (HTTP 200)

**Total reprompts for deploy:** 4

---

*Browser testing to be done in separate grading session.*
