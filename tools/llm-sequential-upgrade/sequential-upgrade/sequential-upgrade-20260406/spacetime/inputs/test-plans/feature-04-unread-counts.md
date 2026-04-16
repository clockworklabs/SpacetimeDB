# Feature 4: Unread Message Counts

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Both users in the same room ("General")
- A second room exists (create "Random" if needed)

## Test Steps

### Step 1: Unread Badge
1. **Tab A**: Navigate to/join a second room ("Random") so Alice is NOT viewing "General".
2. **Tab B**: Send 3 messages in "General": "msg1", "msg2", "msg3".
3. **Tab A**: Check the room list for a badge/count on "General". Use `get_page_text` and look for "(3)" or a number badge near "General".

**Criterion:** Unread count badge shows on room list (1 point)

### Step 2: Per-User Tracking
1. **Tab A**: Click on "General" to open it and read the messages.
2. **Verify**: The unread badge should clear (0 or disappear).
3. **Tab B**: Send one more message in "General".
4. **Tab A**: Navigate back to "Random" or check the room list.
5. **Verify**: Badge should show "(1)" — only the new unread message.

**Criterion:** Count tracks last-read position per user per room (1 point)

### Step 3: Real-Time Updates
1. **Tab A**: Stay on "Random" (not viewing "General").
2. **Tab B**: Send another message to "General".
3. **Tab A**: Watch the room list — the badge count should increment in real-time without refresh.
4. Use `get_page_text` to verify the count updated.

**Criterion:** Counts update in real-time (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Counts work but don't update in real-time (need refresh) |
| 1 | Badge shows but count is incorrect |
| 0 | Not implemented |

## Evidence
- Screenshot of room list showing unread badge with count
