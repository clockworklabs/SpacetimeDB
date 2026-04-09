# Feature 3: Read Receipts

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Both users in the same room
- At least one message has been sent

## Test Steps

### Step 1: Track Message Views
1. **Tab A**: Send a new message "Can you see this?".
2. **Tab A**: Check if the message shows any "unseen" or "sent" indicator (checkmark, etc.).

**Criterion:** System tracks which users have seen which messages (1 point)

### Step 2: Display Seen Indicator
1. **Switch to Tab B**: The room should already be open (or navigate to it).
2. Scroll to or view the message from Alice.
3. **Switch to Tab A**: Check under Alice's message for "Seen by Bob" or similar text.
4. Use `get_page_text` and search for "Seen by" or "seen" or "read".

**Criterion:** "Seen by X, Y, Z" displays under messages (1 point)

### Step 3: Real-Time Update
1. Open a **Tab C** (if not already open) with a third user "Charlie".
2. Have Charlie join the same room and view the message.
3. **Switch to Tab A**: The seen indicator should update to include Charlie WITHOUT a page refresh.
4. Look for "Seen by Bob, Charlie" or "Seen by 2" etc.

**Criterion:** Read status updates in real-time (1 point)

### Fallback (if 3rd user not practical)
- Instead of Tab C, verify that Tab A sees the "Seen by Bob" indicator appear in real-time (without refresh) after switching to Tab B and back.

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, real-time updates |
| 2 | Works but laggy or shows only "seen" without names |
| 1 | Read state tracked but not displayed properly |
| 0 | Not implemented |

## Evidence
- Screenshot of message showing "Seen by Bob" indicator
