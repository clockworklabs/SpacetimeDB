# Feature 2: Typing Indicators

**Max Score: 3** | **Multi-user: Yes + Timing**

## Preconditions
- Both users (Alice in Tab A, Bob in Tab B) are in the same room
- Messages have been sent successfully (Feature 1 passes)

## Test Steps

### Step 1: Typing Broadcast
1. **Tab A**: Click on the message input field.
2. Type a few characters slowly (do NOT press Enter). Use `computer` to type "typing test".
3. **Switch to Tab B** immediately.
4. **Verify Tab B**: Look for "Alice is typing..." or similar indicator. Use `get_page_text` and search for "typing" or "is typing".

**Criterion:** Typing state is broadcast to other room members (1 point)

### Step 2: Auto-Expiry
1. **Tab A**: Stop typing (do nothing for 5-6 seconds).
2. Wait 6 seconds. Use `computer(action: "wait", duration: 6000)` or equivalent.
3. **Switch to Tab B**.
4. **Verify Tab B**: The typing indicator should be gone. Use `get_page_text` — "typing" text should no longer appear.

**Criterion:** Typing indicator auto-expires after inactivity (1 point)

### Step 3: UI Display
1. **Tab A**: Start typing again.
2. **Tab B**: Also start typing in the same room.
3. **Tab A**: Check for "Bob is typing..." or "Multiple users are typing..." text.

**Criterion:** UI shows appropriate typing message (1 point)

### Step 4: Clear on Send
1. **Tab A**: Type a message and press Enter to send.
2. **Switch to Tab B**: Verify the typing indicator clears immediately (not waiting for timeout).

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met, updates in real-time |
| 2 | Works but noticeable delay or missing multi-user display |
| 1 | Typing tracked but doesn't expire or UI is broken |
| 0 | Not implemented |

## Timing Notes
- The auto-expiry test (Step 2) requires a 5-6 second wait. This is the minimum wait time.
- If using `gif_creator`, start recording before Tab A types and stop after verifying expiry in Tab B.

## Evidence
- Screenshot of Tab B showing "Alice is typing..."
- Screenshot of Tab B after timeout (indicator gone)
