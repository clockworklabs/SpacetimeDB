# Iteration Log

## Run Info
- **Backend:** spacetime
- **Level:** 7
- **Started:** 2026-04-02T11:49:31
- **Module:** chat-app-20260402-114931

---

## Iteration 0 — Initial Deploy (successful, no build reprompts)

**Status:** Deploy complete, dev server running at http://localhost:5173

**Backend:** Published to SpacetimeDB local server
- Module: chat-app-20260402-114931
- Tables: user, room, room_member, message, typing_indicator, message_read, room_read_position, message_reaction, message_edit_history, scheduled_message (public); ephemeral_delete_timer, typing_cleanup_timer, activity_check_timer (private scheduled)
- Reducers: register, setStatus, updateActivity, createRoom, joinRoom, leaveRoom, sendMessage, editMessage, setTyping, markMessageRead, markRoomRead, toggleReaction, scheduleMessage, cancelScheduledMessage, kickUser, banUser, promoteUser

**Client:** Built successfully, TypeScript type-check passed, Vite build clean
- Dev server: http://localhost:5173

**Build reprompts:** 0
**Features implemented:**
1. Basic Chat - users, rooms, messages, online status
2. Typing Indicators - server-side 6s expiry + cleanup timer
3. Read Receipts - "Seen by X" per message
4. Unread Message Counts - badges on room list
5. Scheduled Messages - datetime picker, cancel option
6. Ephemeral Messages - 30s/1m/5m/1h countdown
7. Message Reactions - 6 emojis, toggle, hover tooltip
8. Message Editing with History - inline edit, view history
9. Real-Time Permissions - kick/ban/promote for admins
10. Rich User Presence - status (online/away/dnd/invisible), lastActive display, auto-away via timer
