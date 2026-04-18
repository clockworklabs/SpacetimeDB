# Feature 21: Slow Mode

**Max Score: 3** | **Multi-user: Yes (2 tabs)** | **Timing: Yes**

## Preconditions
- Alice is an admin of a room, Bob is a regular member

## Test Steps

### Step 1: Enable Slow Mode
1. **Tab A (Alice)**: As admin, look for a channel settings or slow mode toggle.
2. Enable slow mode with a short cooldown (e.g., 10 seconds).
3. **Verify Tab A**: A "Slow Mode" indicator appears in the channel header.
4. **Verify Tab B**: Bob also sees the slow mode indicator in real-time.

**Criterion:** Admins can enable slow mode, indicator visible to all members (1 point)

### Step 2: Cooldown Enforcement
1. **Tab B (Bob)**: Send a message — should succeed.
2. Immediately try to send another message.
3. **Verify**: Either the input is disabled with a countdown timer, or the send is rejected with feedback showing remaining cooldown.
4. Wait for the cooldown to expire, then send again — should succeed.

**Criterion:** Cooldown enforced for regular users with visual feedback (1 point)

### Step 3: Admin Exemption
1. **Tab A (Alice)**: Send two messages in rapid succession.
2. **Verify**: Both succeed — admins are exempt from slow mode.

**Criterion:** Admins are exempt from slow mode restrictions (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass — enable, enforce with UI feedback, admin exempt |
| 2 | Slow mode enforced but missing UI feedback or admin exemption |
| 1 | Setting exists but enforcement is broken |
| 0 | Not implemented |
