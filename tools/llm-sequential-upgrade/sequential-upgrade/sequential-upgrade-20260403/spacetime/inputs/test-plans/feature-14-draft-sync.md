# Feature 14: Draft Sync

**Max Score: 3** | **Multi-user: Single tab OK for basic, multi for sync**

## Preconditions
- User is logged in and in a room
- Multiple rooms exist for room-switching test

## Test Steps

### Step 1: Auto-Save Draft
1. **Tab A**: In "General" room, start typing "This is a draft..." in the message input but do NOT send.
2. Switch to a different room ("Random") by clicking on it.
3. Switch back to "General".
4. **Verify**: The draft text "This is a draft..." is still in the message input. Use `find("message input")` and check its value, or `get_page_text`.

**Criterion:** Message drafts save automatically as user types (1 point)

### Step 2: Cross-Session Sync
1. Open a new tab (Tab C or Tab B if same user) logged in as the same user (Alice).
2. Navigate to "General" room.
3. **Verify**: The draft "This is a draft..." appears in the message input of the new tab.
4. Update the draft in one tab → verify it updates in the other tab.

**Criterion:** Drafts sync across devices/sessions in real-time (1 point)

### Step 3: Per-Room Drafts
1. **Tab A**: Switch to "Random" room and type a different draft "Random draft".
2. Switch back to "General" → verify "This is a draft..." is still there.
3. Switch to "Random" → verify "Random draft" is still there.

**Criterion:** Each room maintains its own draft per user (0.5 points)

### Step 4: Clear on Send
1. **Tab A**: In "General", send the draft message (press Enter).
2. **Verify**: The input clears after sending.
3. Switch to another room and back to "General" — no draft should be saved.

**Criterion:** Drafts persist until sent or manually cleared (0.5 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Drafts save locally but don't sync across sessions |
| 1 | Drafts exist but are lost on room switch or page refresh |
| 0 | Not implemented |

## Evidence
- Screenshot showing draft preserved after room switch
