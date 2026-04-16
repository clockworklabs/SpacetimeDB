# Feature 5: Scheduled Messages

**Max Score: 3** | **Multi-user: Yes + Timing (20s+ wait)**

## Preconditions
- Both users in the same room
- Basic messaging works

## Test Steps

### Step 1: Schedule a Message
1. **Tab A**: Look for a scheduling option — could be a clock icon, "schedule" button, or menu option near the message input. Use `find("schedule")` or `find("clock")` or `find("later")`.
2. Compose a message like "Scheduled hello!".
3. Set the delivery time to the shortest available (e.g., 10-15 seconds from now, or 1 minute).
4. Submit/schedule it.
5. **Verify Tab B**: The message should NOT appear in the chat yet. Use `get_page_text` to confirm "Scheduled hello!" is absent.

**Criterion:** Users can compose and schedule messages for future delivery (1 point)

### Step 2: Pending Message UI
1. **Tab A**: Look for a "pending" or "scheduled" section/indicator. Use `find("pending")` or `find("scheduled")`.
2. **Verify**: The scheduled message appears with a cancel option.
3. If there's a second message to test cancel: schedule another, then cancel it, verify it doesn't appear.

**Criterion:** Pending scheduled messages visible to author with cancel option (1 point)

### Step 3: Delivery
1. Wait for the scheduled time to arrive. If the minimum was 10 seconds, wait 15-20 seconds. If 1 minute, wait 65 seconds.
2. **Tab B**: Check if "Scheduled hello!" now appears in the chat. Use `get_page_text`.
3. **Tab A**: Also verify the message appears in the normal chat flow.

**Criterion:** Message appears in room at scheduled time (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, timing accurate (within a few seconds) |
| 2 | Works but timing is off or cancel doesn't work |
| 1 | Can schedule but messages never appear or appear immediately |
| 0 | Not implemented |

## Timing Notes
- This test requires waiting for the scheduled delivery time. Budget 20-65 seconds depending on the minimum scheduling interval.
- If the app only allows scheduling 1+ minute out, this test will take over a minute.

## Evidence
- Screenshot of pending scheduled message with cancel option
- Screenshot of delivered message visible to both users
