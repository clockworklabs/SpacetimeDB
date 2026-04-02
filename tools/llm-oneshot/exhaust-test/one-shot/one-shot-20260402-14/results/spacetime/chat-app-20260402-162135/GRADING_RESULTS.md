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
| **Total Feature Score** | 12 / 66    |

---

## Feature 1: Basic Chat (Score: 3 / 3)

- [x] users can set a display name (expected, 36ms)
- [x] users can create and join rooms (expected, 110ms)
- [x] messages appear in real-time for all users (expected, 2513ms)
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

## Feature 9: Real-Time Permissions (Score: 1 / 3)

- [x] room creator has admin controls visible (expected, 16ms)
- [x] non-admin does not have admin controls (expected, 6ms)
- [x] admin can promote another user to admin (expected, 2151ms)
- [ ] admin can kick a user and they lose access immediately (unexpected, 4578ms)
- [ ] permission changes apply in real-time without refresh (unexpected, 2306ms)

---

## Feature 10: Rich User Presence (Score: 2 / 3)

- [x] status selector UI exists with multiple status options (expected, 22ms)
- [ ] user can change status to away (unexpected, 47ms)
- [x] status change syncs to other users in real-time (expected, 18ms)
- [x] user can set do-not-disturb status (expected, 1037ms)
- [ ] last active timestamp for offline users (unexpected, 2084ms)
- [x] auto-away UI mechanism exists (expected, 18ms)

---

## Feature 11: Message Threading (Score: 3 / 3)

- [x] reply button appears on message hover and opens thread (expected, 1147ms)
- [x] can send a reply in the thread (expected, 179ms)
- [x] reply count badge appears on parent message (expected, 17ms)
- [x] other user sees reply count update in real-time (expected, 11ms)
- [x] thread panel shows parent message and all replies (expected, 2394ms)
- [x] thread replies sync in real-time to other viewers (expected, 1338ms)

---

## Feature 12: Private Rooms & DMs (Score: 1 / 3)

- [x] can create a private room with privacy toggle (expected, 115ms)
- [ ] private room is hidden from non-members (unexpected, 2102ms)
- [ ] can invite a user to private room (unexpected, 60054ms)
- [ ] invited user can accept and access private room (unexpected, 60052ms)
- [x] non-invited users still cannot see private room (expected, 2032ms)
- [ ] direct message between users works (unexpected, 60042ms)

---

## Feature 13: Room Activity Indicators (Score: 1 / 3)

- [ ] sending a message shows an activity badge on the room (unexpected, 10224ms)
- [ ] rapid messages trigger a "Hot" badge (unexpected, 6590ms)
- [x] activity badges are visible to both users (expected, 3046ms)
- [x] activity indicators update in real-time (expected, 5513ms)

---

## Feature 14: Draft Sync (Score: 1 / 3)

- [ ] draft is preserved when switching rooms (unexpected, 3089ms)
- [x] different rooms maintain separate drafts (expected, 6217ms)
- [ ] draft persists after page refresh (cross-session) (unexpected, 5397ms)
- [x] draft clears after sending the message (expected, 6463ms)
- [ ] draft syncs across sessions in real-time (unexpected, 8052ms)

---

## Feature 15: Anonymous to Registered Migration (Score: 0 / 3)

- [x] anonymous user gets auto-generated name on first visit (expected, 2638ms)
- [ ] anonymous user can send messages with attribution (unexpected, 60035ms)
- [ ] anonymous session persists on refresh (unexpected, 3ms)
- [ ] registration migrates anonymous messages to new name (unexpected, 3ms)
- [ ] room membership preserved after registration (unexpected, 2ms)

---

## Feature 16: Pinned Messages (Score: 0 / 3)

- [ ] should pin a message and show pin indicator (unexpected, 60046ms)
- [ ] should display pinned messages in the pinned panel (unexpected, 5058ms)
- [ ] should unpin a message and remove it from the panel (unexpected, 60046ms)
- [ ] should sync pin/unpin actions in real-time across clients (unexpected, 60035ms)

---

## Feature 17: User Profiles (Score: 0 / 3)

- [ ] should edit profile bio and status (unexpected, 5073ms)
- [ ] should show profile card when clicking a username (unexpected, 12220ms)
- [ ] should propagate name changes in real-time across all views (unexpected, 6754ms)
- [ ] should display updated profile info in the profile card (unexpected, 5066ms)

---

## Feature 18: @Mentions and Notifications (Score: 0 / 3)

- [ ] should highlight @mentions in message text (unexpected, 60037ms)
- [x] should show notification bell with unread count (expected, 25ms)
- [ ] should display mentions in the notification panel (unexpected, 5058ms)
- [ ] should mark notifications as read (unexpected, 4109ms)
- [ ] should update notifications in real-time (unexpected, 60052ms)

---

## Feature 19: Bookmarked/Saved Messages (Score: 0 / 3)

- [ ] users can bookmark messages (unexpected, 60036ms)
- [ ] saved messages panel shows bookmarks with context (unexpected, 5051ms)
- [ ] remove bookmark works and bookmarks are private (unexpected, 5053ms)

---

## Feature 20: Message Forwarding (Score: 0 / 3)

- [ ] forward button opens channel picker and sends (unexpected, 6561ms)
- [ ] forwarded message shows in target with attribution (unexpected, 10258ms)
- [ ] original message not modified by forwarding (unexpected, 6123ms)

---

## Feature 21: Slow Mode (Score: 0 / 3)

- [ ] admins can enable slow mode with visible indicator (unexpected, 5065ms)
- [ ] cooldown enforced for regular users with UI feedback (unexpected, 3199ms)
- [ ] admins are exempt from slow mode (unexpected, 60044ms)

---

## Feature 22: Polls (Score: 0 / 3)

- [ ] create poll with question and options (unexpected, 5086ms)
- [ ] votes update in real-time and changing vote is atomic (unexpected, 5054ms)
- [ ] close poll and voter names visible (unexpected, 60042ms)


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
| 9. Real-Time Permissions | 3 | 1 | 3/5 passed, 0 skipped |
| 10. Rich User Presence | 3 | 2 | 4/6 passed, 0 skipped |
| 11. Message Threading | 3 | 3 | 6/6 passed, 0 skipped |
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
| **TOTAL** | **66** | **12** | |
