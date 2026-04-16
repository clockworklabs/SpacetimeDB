# Bug Report

## Bug 1: Unread message count badge not showing

**Feature:** Unread Message Counts (Feature 4)
**Severity:** Critical — feature completely non-functional

**Steps to reproduce:**
1. Open two tabs (Alice and Bob)
2. Both join the "General" room
3. Bob navigates to a different room (or has no room selected)
4. Alice sends a message to General
5. Expected: a numeric badge appears next to "General" in Bob's room list
6. Actual: no badge appears — unread counts are not displayed

**Acceptance criteria:**
- Numeric badge (e.g. "1", "2", "3") appears next to room name in sidebar when there are unread messages
- Badge count increments with each new message
- Badge disappears when the room is opened/entered
