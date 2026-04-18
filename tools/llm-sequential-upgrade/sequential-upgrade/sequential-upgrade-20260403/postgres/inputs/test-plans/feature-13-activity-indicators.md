# Feature 13: Room Activity Indicators

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Multiple rooms exist
- At least one room has recent messages

## Test Steps

### Step 1: Activity Badge
1. **Tab A**: Check the room list for activity indicators — look for "Active", "Hot", fire icon, or colored indicators on rooms. Use `get_page_text` and search for "active" or "hot".
2. A room with recent messages should show some activity badge.

**Criterion:** Activity badges show on rooms (1 point)

### Step 2: Message Velocity
1. **Tab B**: Send 10+ messages rapidly in one room (quick succession).
2. **Tab A**: Check if the activity indicator changes to reflect higher activity (e.g., "Active" → "Hot", or a more prominent indicator).

**Criterion:** Activity level reflects recent message velocity (1 point)

### Step 3: Real-Time Update
1. Wait a few minutes without activity in the room.
2. Check if the activity indicator decreases or changes to reflect lower activity.
3. Alternatively: send messages again and verify the indicator updates in real-time.

**Criterion:** Indicators update in real-time as activity changes (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Shows activity but doesn't update in real-time |
| 1 | Static badge that doesn't reflect actual activity |
| 0 | Not implemented |

## Evidence
- Screenshot of room list showing activity indicators
