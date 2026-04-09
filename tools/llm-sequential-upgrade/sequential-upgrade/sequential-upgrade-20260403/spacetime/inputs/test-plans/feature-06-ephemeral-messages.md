# Feature 6: Ephemeral/Disappearing Messages

**Max Score: 3** | **Multi-user: Yes + Timing**

## Preconditions
- Both users in the same room
- Basic messaging works

## Test Steps

### Step 1: Send Ephemeral Message
1. **Tab A**: Look for an ephemeral/disappearing message option. Use `find("ephemeral")` or `find("disappearing")` or `find("timer")` or `find("self-destruct")`.
2. Set the shortest available timer (e.g., 30 seconds or 1 minute).
3. Send a message like "This will disappear!".
4. **Verify Tab B**: Message appears in Tab B's chat. Use `get_page_text` to confirm.

**Criterion:** Users can send messages with auto-delete timer (1 point)

### Step 2: Countdown Indicator
1. **Tab A or Tab B**: Look for a visual countdown or timer indicator on the ephemeral message.
2. Use `get_page_text` or `find("countdown")` or look for a timer icon/number.

**Criterion:** Countdown or disappearing indicator shown in UI (1 point)

### Step 3: Deletion
1. Wait for the timer to expire (30-65 seconds depending on minimum timer).
2. **Tab A**: Check if the message is gone. Use `get_page_text` — "This will disappear!" should not be found.
3. **Tab B**: Also verify the message is gone.

**Criterion:** Message is permanently deleted when timer expires (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, deletion on schedule |
| 2 | Messages delete but no visual countdown, or timing inaccurate |
| 1 | Option exists but messages don't actually delete |
| 0 | Not implemented |

## Timing Notes
- This test requires waiting for the ephemeral timer to expire. Budget 30-65 seconds.
- Use `gif_creator` if you want to capture the countdown and disappearance.

## Evidence
- Screenshot of ephemeral message with countdown visible
- Screenshot after expiry showing message is gone
