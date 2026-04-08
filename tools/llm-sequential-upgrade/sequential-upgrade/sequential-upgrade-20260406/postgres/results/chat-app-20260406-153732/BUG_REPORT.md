# Bug Report — L10 Activity Indicators + L8 Threading (PostgreSQL)

## Bug 1: Activity indicators do not reset in real-time

When a room becomes inactive (no new messages), the activity badge (e.g. "Hot", "Active") does not update for connected clients until they refresh the page.

**Expected:** Activity level should decrease/reset automatically in real-time as rooms become less active, without requiring a page refresh. This likely requires a server-side timer or scheduled job that recalculates activity levels and emits a Socket.io event to update connected clients.

## Bug 2: Thread unread badge does not update in real-time

When a new message is posted in a thread, the unread message badge on the room does not update in real-time for other connected users. The badge only reflects thread messages after a page refresh.

**Expected:** Thread replies should be counted toward the room's unread badge and the count should update via Socket.io in real-time, the same way top-level messages do. When a thread reply is sent, emit the same unread count update event that regular messages trigger.
