# Chat App Grading Results

**Model:** Playwright (automated)
**Date:** 2026-04-02
**Backend:** spacetime
**Grading Method:** Playwright automated tests

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Features Evaluated**  | 1-15                           |
| **Total Feature Score** | 5 / 66    |

---

## Feature 1: Basic Chat (Score: 3 / 3)

- [x] users can set a display name (expected, 21ms)
- [x] users can create and join rooms (expected, 145ms)
- [x] messages appear in real-time for all users (expected, 351ms)
- [x] online user list shows connected users (expected, 9ms)

---

## Feature 2: Typing Indicators (Score: 0 / 3)

- [ ] typing state broadcasts to other users (unexpected, 0ms)
- [ ] typing indicator auto-expires after inactivity (skipped, 0ms)
- [ ] typing indicator displays correctly in UI (skipped, 0ms)

---

## Feature 3: Read Receipts (Score: 0 / 3)

- [ ] seen-by indicator displays under messages after recipient views (unexpected, 0ms)
- [ ] read status includes the reader name (skipped, 0ms)
- [ ] read status updates in real-time when another user views (skipped, 0ms)

---

## Feature 4: Unread Counts (Score: 0 / 3)

- [ ] unread count badge appears when messages arrive in another room (unexpected, 0ms)
- [ ] badge clears when room is opened (skipped, 0ms)
- [ ] counts are per-user — Alice does not see unread for her own messages (skipped, 0ms)

---

## Feature 5: Scheduled Messages (Score: 0 / 3)

- [ ] schedule button is accessible from the message input area (unexpected, 0ms)
- [ ] can schedule a message for the future (skipped, 0ms)
- [ ] pending scheduled messages are visible to author with cancel option (skipped, 0ms)
- [ ] scheduled message is NOT visible to other users before delivery time (skipped, 0ms)

---

## Feature 6: Ephemeral Messages (Score: 0 / 3)

- [ ] can send an ephemeral/disappearing message with duration (unexpected, 0ms)
- [ ] ephemeral message shows countdown or disappearing indicator (skipped, 0ms)
- [ ] both users see the ephemeral message (skipped, 0ms)
- [ ] ephemeral message disappears after the duration expires (skipped, 0ms)

---

## Feature 7: Message Reactions (Score: 0 / 3)

- [ ] can add a reaction to a message (unexpected, 0ms)
- [ ] reaction count appears and is visible to both users (skipped, 0ms)
- [ ] can toggle reaction off — count decreases or disappears (skipped, 0ms)
- [ ] multiple users can react and counts aggregate (skipped, 0ms)

---

## Feature 8: Message Editing with History (Score: 0 / 3)

- [ ] can edit own message (unexpected, 0ms)
- [ ] edited indicator appears on edited messages (skipped, 0ms)
- [ ] other user sees edit in real-time (skipped, 0ms)
- [ ] edit history is viewable by clicking the edited indicator (skipped, 0ms)
- [ ] multiple edits are tracked in history (skipped, 0ms)

---

## Feature 9: Real-Time Permissions (Score: 0 / 3)

- [ ] room creator has admin controls visible (unexpected, 66ms)
- [x] non-admin does not have admin controls (expected, 7ms)
- [ ] admin can promote another user to admin (unexpected, 2136ms)
- [ ] admin can kick a user and they lose access immediately (unexpected, 2541ms)
- [ ] permission changes apply in real-time without refresh (unexpected, 2134ms)

---

## Feature 10: Rich User Presence (Score: 2 / 3)

- [x] status selector UI exists with multiple status options (expected, 21ms)
- [ ] user can change status to away (unexpected, 56ms)
- [x] status change syncs to other users in real-time (expected, 16ms)
- [x] user can set do-not-disturb status (expected, 1037ms)
- [ ] last active timestamp for offline users (unexpected, 2058ms)
- [x] auto-away UI mechanism exists (expected, 11ms)

---

## Feature 11: Message Threading (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 12: Private Rooms & DMs (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 13: Room Activity Indicators (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 14: Draft Sync (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 15: Anonymous to Registered Migration (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 16: Pinned Messages (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 17: User Profiles (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 18: @Mentions and Notifications (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 19: Bookmarked/Saved Messages (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 20: Message Forwarding (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 21: Slow Mode (Score: 0 / 3)

- [ ] No tests ran

---

## Feature 22: Polls (Score: 0 / 3)

- [ ] No tests ran


---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 3 | 4/4 passed, 0 skipped |
| 2. Typing Indicators | 3 | 0 | 0/1 passed, 2 skipped |
| 3. Read Receipts | 3 | 0 | 0/1 passed, 2 skipped |
| 4. Unread Counts | 3 | 0 | 0/1 passed, 2 skipped |
| 5. Scheduled Messages | 3 | 0 | 0/1 passed, 3 skipped |
| 6. Ephemeral Messages | 3 | 0 | 0/1 passed, 3 skipped |
| 7. Message Reactions | 3 | 0 | 0/1 passed, 3 skipped |
| 8. Message Editing with History | 3 | 0 | 0/1 passed, 4 skipped |
| 9. Real-Time Permissions | 3 | 0 | 1/5 passed, 0 skipped |
| 10. Rich User Presence | 3 | 2 | 4/6 passed, 0 skipped |
| 11. Message Threading | 3 | 0 | No tests |
| 12. Private Rooms & DMs | 3 | 0 | No tests |
| 13. Room Activity Indicators | 3 | 0 | No tests |
| 14. Draft Sync | 3 | 0 | No tests |
| 15. Anonymous to Registered Migration | 3 | 0 | No tests |
| 16. Pinned Messages | 3 | 0 | No tests |
| 17. User Profiles | 3 | 0 | No tests |
| 18. @Mentions and Notifications | 3 | 0 | No tests |
| 19. Bookmarked/Saved Messages | 3 | 0 | No tests |
| 20. Message Forwarding | 3 | 0 | No tests |
| 21. Slow Mode | 3 | 0 | No tests |
| 22. Polls | 3 | 0 | No tests |
| **TOTAL** | **66** | **5** | |
