# Feature 18: @Mentions and Notification Feed

**Max Score: 3** | **Multi-user: Yes (2 tabs)**

## Preconditions
- Both users (Alice in Tab A, Bob in Tab B) are registered and in the same room
- Both users' display names are known to each other

## Test Steps

### Step 1: Send an @Mention
1. **Tab A (Alice)**: In the message input, type `@Bob hello!` and send.
2. **Verify Tab A**: The message appears with "Bob" highlighted/styled differently (bold, colored, or linked).
3. **Verify Tab B**: Bob also sees the message with his name highlighted.

**Criterion:** @mentions are parsed and highlighted in messages (1 point)

### Step 2: Notification Bell
1. **Tab B (Bob)**: Look for a notification bell icon in the sidebar or header.
2. **Verify**: The bell shows an unread count (1 or a dot indicator).
3. Click the bell to open the notification panel.
4. **Verify**: The panel lists a notification for the mention — showing the message text, channel name, and who mentioned Bob.

**Criterion:** Notification bell with count, panel shows mention details (1 point)

### Step 3: Mark as Read and Real-Time Updates
1. **Tab B (Bob)**: Mark the notification as read (click it, or use a "mark read" button).
2. **Verify**: Notification count decreases or clears.
3. **Tab A (Alice)**: Send another message mentioning `@Bob check this out`.
4. **Verify Tab B**: Bob's notification count increments in real-time without page refresh.
5. Clicking the notification should navigate to or highlight the source message.

**Criterion:** Mark as read works, new notifications arrive in real-time, clicking navigates to source (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass — highlight, bell with count, panel, mark read, real-time, navigation |
| 2 | Mentions and notifications work but missing real-time updates or navigation |
| 1 | @mentions are highlighted but no notification system |
| 0 | Not implemented |

## Evidence
- Screenshot of highlighted @mention in message
- Screenshot of notification bell with count
- Screenshot of notification panel listing mentions
