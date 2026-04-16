# Feature 20: Message Forwarding

**Max Score: 3** | **Multi-user: Yes (2 tabs)**

## Preconditions
- Both users in a room with messages
- At least 2 rooms exist

## Test Steps

### Step 1: Forward a Message
1. **Tab A (Alice)**: Hover over a message. Look for a "Forward" button/icon.
2. Click Forward — a channel picker should appear listing channels Alice is a member of.
3. Select a different channel and confirm.
4. **Verify**: Success feedback (toast, confirmation).

**Criterion:** Forward button opens channel picker and sends (1 point)

### Step 2: Forwarded Message Appears
1. **Tab A (Alice)**: Navigate to the target channel.
2. **Verify**: The forwarded message appears with attribution — "Forwarded from #original-channel by Alice" or similar.
3. **Tab B (Bob)**: If Bob is in the target channel, verify the forwarded message appeared in real-time.

**Criterion:** Forwarded message shows in target channel with attribution, real-time sync (1 point)

### Step 3: Original Unchanged
1. **Tab A (Alice)**: Navigate back to the original channel.
2. **Verify**: The original message is unchanged — no "forwarded" indicator on the source.

**Criterion:** Original message not modified by forwarding (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass |
| 2 | Forward works but missing attribution or real-time |
| 1 | Forward button exists but message doesn't appear correctly |
| 0 | Not implemented |
