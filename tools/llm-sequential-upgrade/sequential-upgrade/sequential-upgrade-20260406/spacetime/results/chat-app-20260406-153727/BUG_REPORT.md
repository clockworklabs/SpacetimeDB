# Bug Report

## Bug 1: Ephemeral messages do not appear after sending

**Feature:** Ephemeral Messages (Feature 6)
**Severity:** Critical — feature non-functional

**Steps to reproduce:**
1. Join a room
2. Send an ephemeral message (using whatever UI was added for ephemeral mode)
3. Expected: the message appears in chat (marked as ephemeral) and then disappears after the set duration
4. Actual: the message never appears at all — not visible to sender or other users

**Note:** Regular (non-ephemeral) messages still work correctly.

**Fix required:** Debug why ephemeral messages are not being stored or broadcast. Check the reducer/subscription path for ephemeral messages vs regular messages.
