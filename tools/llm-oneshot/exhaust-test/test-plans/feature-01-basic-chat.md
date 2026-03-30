# Feature 1: Basic Chat Features

**Max Score: 3** | **Multi-user: Yes (2 tabs)**

## Preconditions
- App is running at http://localhost:5173
- Two browser tabs open (Tab A, Tab B)

## Test Steps

### Step 1: Set Display Names
1. **Tab A**: Look for a name input or "set name" prompt. Use `find("name input")` or `find("display name")`.
2. Enter "Alice" and submit.
3. **Tab B**: Enter "Bob" and submit.
4. **Verify**: Both names appear in the UI. Use `get_page_text` to confirm "Alice" and "Bob" are visible.

**Criterion:** Users can set a display name (0.5)

### Step 2: Create a Chat Room
1. **Tab A**: Find the "create room" button or form. Use `find("create room")` or `find("new room")`.
2. Enter room name "General" and create.
3. **Verify Tab A**: "General" appears in the room list. Use `get_page_text` to confirm.
4. **Verify Tab B**: "General" also appears in Tab B's room list (real-time update).

**Criterion:** Users can create chat rooms (0.5)

### Step 3: Join Room
1. **Tab A**: Click/join "General" room.
2. **Tab B**: Click/join "General" room.
3. **Verify**: Both users appear in the room's member/online list. Look for "Alice" and "Bob" in the member panel.

**Criterion:** Users can join/leave rooms (0.5) + Online users are displayed (0.5)

### Step 4: Send Messages
1. **Tab A**: Find the message input. Use `find("message input")` or `find("type a message")`.
2. Type "Hello from Alice!" and press Enter (or click send).
3. **Verify Tab A**: Message appears in the chat.
4. **Switch to Tab B**: Verify "Hello from Alice!" appears in Tab B's chat. Use `get_page_text`.
5. **Tab B**: Send "Hi Alice, this is Bob!".
6. **Switch to Tab A**: Verify Bob's message appears.

**Criterion:** Users can send messages to joined rooms (0.5)

### Step 5: Validation
1. **Tab A**: Try to send an empty message (press Enter with no text).
2. **Verify**: Message is either rejected or not sent. Check that no empty message appears in the chat.

**Criterion:** Basic validation exists (0.5)

### Step 6: Leave Room (if applicable)
1. **Tab B**: Find a "leave room" button if one exists. Use `find("leave")`.
2. If found, click it and verify Tab B no longer sees new messages in that room.

**Criterion:** Users can join/leave rooms (0.5) — partial credit if join works but leave doesn't

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All 6 criteria pass |
| 2 | 4-5 criteria pass |
| 1 | 2-3 criteria pass |
| 0 | 0-1 criteria pass |

## Evidence
- Screenshot after Step 4 (messages visible in both tabs)
- Screenshot of room list showing the created room
