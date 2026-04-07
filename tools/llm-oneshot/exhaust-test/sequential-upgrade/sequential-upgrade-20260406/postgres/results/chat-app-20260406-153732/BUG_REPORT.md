# Bug Report — L7 Presence (PostgreSQL)

## Bug 1: "Last active" timestamp is always wrong

When a user sets their status to invisible/offline, the "Last active X minutes ago" display shows a wildly inaccurate time — e.g. "15 minutes ago" immediately after the user was just active seconds ago.

**Root cause (likely):** The `last_active_at` timestamp on the user record is not being updated when the user is active (e.g. on connection, message send, or status change). It may be set only at registration time, or defaulting to a stale value.

**Required fix:**
- Update the user's `last_active_at` timestamp to the current server time whenever the user performs any action (connects, sends a message, changes status, etc.)
- Ensure the "Last active X ago" display computes the diff against the actual current time, not a stale value
