# Bug Report

## Bug 1: Read receipts display sender as a viewer

**Feature:** Read Receipts

**Description:** When user B sends a message, other clients show "Seen by B" beneath that message — the sender's own name appears in the seen-by list. The sender should never appear in the "Seen by" display. Only users OTHER than the message sender should appear.

**Expected:** "Seen by Alice" (only readers who are not the sender)
**Actual:** "Seen by Bob" appears on Bob's own message when viewed by other clients

## Bug 2: No unread message count badges

**Feature:** Unread Message Counts

**Description:** The room list shows no unread count badges when there are unread messages in a room. Badges should appear as a pill-shaped number next to the room name and clear when the room is entered.

## Bug 3: No way to leave a room

**Feature:** Basic Chat

**Description:** There is no "Leave" button or mechanism to leave a room the user has joined. A "Leave" button must be visible when inside a room.
