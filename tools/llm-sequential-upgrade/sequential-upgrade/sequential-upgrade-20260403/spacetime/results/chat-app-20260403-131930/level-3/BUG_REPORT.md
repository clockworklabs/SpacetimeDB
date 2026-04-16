# Bug Report

## Bug 1: Read receipts display sender as a viewer

**Feature:** Read Receipts

**Description:** When user B sends a message, other clients show "Seen by B" beneath that message — the sender's own name appears in the seen-by list. The sender should never appear in the "Seen by" display. Only users OTHER than the message sender should appear.

**Expected:** "Seen by Alice" (only readers who are not the sender)
**Actual:** "Seen by Bob" appears on Bob's own message when viewed by other clients
