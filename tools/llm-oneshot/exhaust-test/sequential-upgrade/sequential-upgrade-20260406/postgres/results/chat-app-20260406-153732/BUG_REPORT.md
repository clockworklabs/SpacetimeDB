# Bug Report — L6 Permissions (PostgreSQL)

## Bug 1: Member panel not updating in real-time when users join/leave rooms

When a user joins or leaves a room, the member/user panel does not update in real-time for other connected clients. A page refresh is required to see the current member list.

**Expected:** Member list updates instantly via Socket.io events for all connected clients in the room.

## Bug 2: Kicked members can still fully use the room (STILL NOT FIXED after 2 attempts)

After being kicked, the user:
- Can click back into the room from the room list
- Can view all messages
- Can send new messages to the room
- Only symptom: they don't appear in the members panel

Two previous fix attempts have not resolved this. The root cause is that the server has no "banned/kicked" state — kicking only removes the room membership row, but the join endpoint re-creates it when the user navigates back to the room.

**Required fix:**
1. Add a `room_bans` table (or a `banned` column on room memberships) to persistently track kicked users
2. The `POST /api/rooms/:id/join` endpoint must check the ban list and return 403 if the user is banned
3. `GET /api/rooms/:id/messages` and `POST /api/rooms/:id/messages` must also check ban status and return 403
4. Emit a Socket.io `kicked_from_room` event to the kicked user's socket so the client navigates away immediately
5. The client must handle the `kicked_from_room` event and remove the room from the UI / deselect it
