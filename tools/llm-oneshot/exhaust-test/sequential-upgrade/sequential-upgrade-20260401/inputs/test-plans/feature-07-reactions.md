# Feature 7: Message Reactions

**Max Score: 3** | **Multi-user: Yes**

## Preconditions
- Both users in the same room
- At least one message exists to react to

## Test Steps

### Step 1: Add Reaction
1. **Tab A**: Find a message from Bob. Look for a reaction button — could be a smiley face icon, "+" button, or hover menu on the message. Use `find("reaction")` or `find("emoji")` or hover over a message.
2. Click the reaction trigger and select an emoji (e.g., thumbs up).
3. **Verify Tab A**: Reaction appears on the message with count "1". Use `get_page_text` to look for the emoji or a count.

**Criterion:** Users can add emoji reactions to messages (0.75 points)

### Step 2: Real-Time Count Update
1. **Switch to Tab B**: Check that Alice's reaction is visible on the message.
2. **Tab B**: Add the same emoji reaction to the same message.
3. **Verify Tab A**: Count updates to "2" in real-time. Use `get_page_text`.

**Criterion:** Reaction counts display and update in real-time (0.75 points)

### Step 3: Toggle Off
1. **Tab A**: Click the same reaction emoji again to remove Alice's reaction.
2. **Verify**: Count decreases to "1" (only Bob's reaction remains).
3. **Tab B**: Verify the count update is reflected.

**Criterion:** Users can toggle their own reactions on/off (0.75 points)

### Step 4: Who Reacted
1. **Tab A or Tab B**: Hover over or click on the reaction to see who reacted.
2. Look for a tooltip or popup showing the reactor's name ("Bob").
3. Use `get_page_text` or `find("tooltip")` after hovering.

**Criterion:** Hover/click shows who reacted (0.75 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All 4 criteria met |
| 2 | Reactions work but missing hover details or toggle buggy |
| 1 | Can react but counts don't update in real-time |
| 0 | Not implemented |

## Evidence
- Screenshot of message with reaction emoji and count
