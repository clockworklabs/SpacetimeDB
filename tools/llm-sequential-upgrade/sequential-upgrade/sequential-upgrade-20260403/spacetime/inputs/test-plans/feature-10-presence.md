# Feature 10: Rich User Presence

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Both users registered and in a room

## Test Steps

### Step 1: Set Status
1. **Tab A**: Look for a status selector — dropdown, menu, or profile settings. Use `find("status")` or `find("away")` or `find("presence")`.
2. Change status to "away" (or equivalent).
3. **Switch to Tab B**: Verify Alice's status indicator changes — look for a yellow/orange dot, "away" text, or changed icon. Use `get_page_text` to search for "away".

**Criterion:** Users can set status: online, away, do-not-disturb, invisible (1 point)

### Step 2: Last Active
1. If Alice sets status to offline/invisible or disconnects, check if "Last active X minutes ago" appears for Alice in Tab B's view. Use `get_page_text` to search for "last active" or "ago".

**Criterion:** "Last active X minutes ago" shows for offline users (0.5 points)

### Step 3: Real-Time Sync
1. **Tab A**: Change status back to "online".
2. **Tab B**: Verify the change appears WITHOUT refreshing. Status indicator should change in real-time.

**Criterion:** Status changes sync to all viewers in real-time (1 point)

### Step 4: Auto-Away
1. This is hard to test with browser tools since it requires several minutes of inactivity.
2. **Alternative**: Use `javascript_tool` to check if there's an auto-away mechanism in the code (look for timers or inactivity listeners).
3. If verifiable: leave Tab A idle for the configured period and check if status changes.

**Criterion:** Auto-set to "away" after inactivity period (0.5 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Manual status works but no auto-away or last-active |
| 1 | Status exists but doesn't sync in real-time |
| 0 | Not implemented |

## Evidence
- Screenshot showing status indicator (colored dot or text)
