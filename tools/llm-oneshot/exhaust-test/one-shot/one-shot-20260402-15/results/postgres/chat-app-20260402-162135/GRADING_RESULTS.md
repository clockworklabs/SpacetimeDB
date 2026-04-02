# Chat App Grading Results

**Model:** Playwright (automated)
**Date:** 2026-04-02
**Backend:** postgres
**Grading Method:** Playwright automated tests

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Features Evaluated**  | 1-15                           |
| **Total Feature Score** | 8 / 66    |

---

## Feature 1: Basic Chat (Score: 1 / 3)

- [x] users can set a display name (expected, 36ms)
- [x] users can create and join rooms (expected, 131ms)
- [ ] messages appear in real-time for all users (unexpected, 10150ms)
- [ ] online user list shows connected users (unexpected, 55ms)

---

## Feature 2: Typing Indicators (Score: 0 / 3)

- [ ] typing state broadcasts to other users (unexpected, 0ms)
- [ ] typing indicator auto-expires after inactivity (skipped, 0ms)
- [ ] typing indicator displays correctly in UI (skipped, 0ms)

---

## Feature 3: Read Receipts (Score: 0 / 3)

- [ ] seen-by indicator displays under messages after recipient views (unexpected, 10079ms)
- [ ] read status includes the reader name (unexpected, 0ms)
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

## Feature 9: Real-Time Permissions (Score: 2 / 3)

- [x] room creator has admin controls visible (expected, 21ms)
- [x] non-admin does not have admin controls (expected, 8ms)
- [x] admin can promote another user to admin (expected, 7073ms)
- [ ] admin can kick a user and they lose access immediately (unexpected, 10265ms)
- [x] permission changes apply in real-time without refresh (expected, 2034ms)

---

## Feature 10: Rich User Presence (Score: 2 / 3)

- [x] status selector UI exists with multiple status options (expected, 18ms)
- [ ] user can change status to away (unexpected, 69ms)
- [x] status change syncs to other users in real-time (expected, 16ms)
- [x] user can set do-not-disturb status (expected, 1022ms)
- [ ] last active timestamp for offline users (unexpected, 2064ms)
- [x] auto-away UI mechanism exists (expected, 16ms)

---

## Feature 11: Message Threading (Score: 0 / 3)

- [ ] reply button appears on message hover and opens thread (unexpected, 0ms)
- [ ] can send a reply in the thread (skipped, 0ms)
- [ ] reply count badge appears on parent message (skipped, 0ms)
- [ ] other user sees reply count update in real-time (skipped, 0ms)
- [ ] thread panel shows parent message and all replies (skipped, 0ms)
- [ ] thread replies sync in real-time to other viewers (skipped, 0ms)

---

## Feature 12: Private Rooms & DMs (Score: 1 / 3)

- [x] can create a private room with privacy toggle (expected, 144ms)
- [ ] private room is hidden from non-members (unexpected, 2073ms)
- [ ] can invite a user to private room (unexpected, 60052ms)
- [ ] invited user can accept and access private room (unexpected, 60061ms)
- [x] non-invited users still cannot see private room (expected, 2023ms)
- [ ] direct message between users works (unexpected, 60057ms)

---

## Feature 13: Room Activity Indicators (Score: 1 / 3)

- [ ] sending a message shows an activity badge on the room (unexpected, 10087ms)
- [ ] rapid messages trigger a "Hot" badge (unexpected, 3276ms)
- [x] activity badges are visible to both users (expected, 2070ms)
- [x] activity indicators update in real-time (expected, 3150ms)

---

## Feature 14: Draft Sync (Score: 1 / 3)

- [ ] draft is preserved when switching rooms (unexpected, 1208ms)
- [x] different rooms maintain separate drafts (expected, 1765ms)
- [ ] draft persists after page refresh (cross-session) (unexpected, 60021ms)
- [x] draft clears after sending the message (expected, 2231ms)
- [ ] draft syncs across sessions in real-time (unexpected, 5477ms)

---

## Feature 15: Anonymous to Registered Migration (Score: 0 / 3)

- [x] anonymous user gets auto-generated name on first visit (expected, 2230ms)
- [ ] anonymous user can send messages with attribution (unexpected, 60033ms)
- [ ] anonymous session persists on refresh (unexpected, 1ms)
- [ ] registration migrates anonymous messages to new name (unexpected, 2ms)
- [ ] room membership preserved after registration (unexpected, 2ms)

---

## Feature 16: Pinned Messages (Score: 0 / 3)

- [ ] should pin a message and show pin indicator (unexpected, 10069ms)
- [ ] should display pinned messages in the pinned panel (unexpected, 5053ms)
- [ ] should unpin a message and remove it from the panel (unexpected, 60035ms)
- [ ] should sync pin/unpin actions in real-time across clients (unexpected, 10079ms)

---

## Feature 17: User Profiles (Score: 0 / 3)

- [ ] should edit profile bio and status (unexpected, 60043ms)
- [ ] should show profile card when clicking a username (unexpected, 10155ms)
- [ ] should propagate name changes in real-time across all views (unexpected, 10129ms)
- [ ] should display updated profile info in the profile card (unexpected, 60043ms)

---

## Feature 18: @Mentions and Notifications (Score: 0 / 3)

- [ ] should highlight @mentions in message text (unexpected, 10066ms)
- [x] should show notification bell with unread count (expected, 15ms)
- [ ] should display mentions in the notification panel (unexpected, 5046ms)
- [ ] should mark notifications as read (unexpected, 5091ms)
- [ ] should update notifications in real-time (unexpected, 9111ms)

---

## Feature 19: Bookmarked/Saved Messages (Score: 0 / 3)

- [ ] users can bookmark messages (unexpected, 10067ms)
- [ ] saved messages panel shows bookmarks with context (unexpected, 5051ms)
- [ ] remove bookmark works and bookmarks are private (unexpected, 5053ms)

---

## Feature 20: Message Forwarding (Score: 0 / 3)

- [ ] forward button opens channel picker and sends (unexpected, 10126ms)
- [ ] forwarded message shows in target with attribution (unexpected, 10103ms)
- [ ] original message not modified by forwarding (unexpected, 5114ms)

---

## Feature 21: Slow Mode (Score: 0 / 3)

- [ ] admins can enable slow mode with visible indicator (unexpected, 5061ms)
- [ ] cooldown enforced for regular users with UI feedback (unexpected, 5083ms)
- [ ] admins are exempt from slow mode (unexpected, 5091ms)

---

## Feature 22: Polls (Score: 0 / 3)

- [ ] create poll with question and options (unexpected, 60045ms)
- [ ] votes update in real-time and changing vote is atomic (unexpected, 5044ms)
- [ ] close poll and voter names visible (unexpected, 60041ms)


---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 1 | 2/4 passed, 0 skipped |
| 2. Typing Indicators | 3 | 0 | 0/1 passed, 2 skipped |
| 3. Read Receipts | 3 | 0 | 0/2 passed, 1 skipped |
| 4. Unread Counts | 3 | 0 | 0/1 passed, 2 skipped |
| 5. Scheduled Messages | 3 | 0 | 0/1 passed, 3 skipped |
| 6. Ephemeral Messages | 3 | 0 | 0/1 passed, 3 skipped |
| 7. Message Reactions | 3 | 0 | 0/1 passed, 3 skipped |
| 8. Message Editing with History | 3 | 0 | 0/1 passed, 4 skipped |
| 9. Real-Time Permissions | 3 | 2 | 4/5 passed, 0 skipped |
| 10. Rich User Presence | 3 | 2 | 4/6 passed, 0 skipped |
| 11. Message Threading | 3 | 0 | 0/1 passed, 5 skipped |
| 12. Private Rooms & DMs | 3 | 1 | 2/6 passed, 0 skipped |
| 13. Room Activity Indicators | 3 | 1 | 2/4 passed, 0 skipped |
| 14. Draft Sync | 3 | 1 | 2/5 passed, 0 skipped |
| 15. Anonymous to Registered Migration | 3 | 0 | 1/5 passed, 0 skipped |
| 16. Pinned Messages | 3 | 0 | 0/4 passed, 0 skipped |
| 17. User Profiles | 3 | 0 | 0/4 passed, 0 skipped |
| 18. @Mentions and Notifications | 3 | 0 | 1/5 passed, 0 skipped |
| 19. Bookmarked/Saved Messages | 3 | 0 | 0/3 passed, 0 skipped |
| 20. Message Forwarding | 3 | 0 | 0/3 passed, 0 skipped |
| 21. Slow Mode | 3 | 0 | 0/3 passed, 0 skipped |
| 22. Polls | 3 | 0 | 0/3 passed, 0 skipped |
| **TOTAL** | **66** | **8** | |
