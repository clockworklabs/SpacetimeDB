# Feature 16: Pinned Messages

**Max Score: 3** | **Multi-user: Yes (2 tabs)**

## Preconditions
- Both users (Alice in Tab A, Bob in Tab B) are in the same room
- Messages have been sent (Feature 1 passes)

## Test Steps

### Step 1: Pin a Message
1. **Tab A (Alice)**: Hover over a message she sent. Look for a pin button/icon.
2. Click the pin button.
3. **Verify Tab A**: Message shows a pin indicator (pin icon, "Pinned" label, or visual highlight).
4. **Verify Tab B**: Bob also sees the pin indicator on the same message in real-time.

**Criterion:** Users can pin messages, pin indicator shows in message list (1 point)

### Step 2: Pinned Messages Panel
1. **Tab A (Alice)**: Look for a "Pinned" or pin icon button in the channel header.
2. Click it to open the pinned messages panel.
3. **Verify**: The pinned message from Step 1 appears in the panel.
4. **Tab B (Bob)**: Also open the pinned messages panel.
5. **Verify**: Bob sees the same pinned message.

**Criterion:** Pinned messages panel accessible from channel header, shows all pinned messages (1 point)

### Step 3: Unpin a Message
1. **Tab A (Alice)**: Unpin the message (via the message hover action or from the pinned panel).
2. **Verify Tab A**: Pin indicator removed from message. Pinned panel is empty or message removed.
3. **Verify Tab B**: Bob's view also updates — pin indicator gone, panel updated in real-time.

**Criterion:** Users can unpin messages, changes sync in real-time (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass — pin, panel, unpin, real-time sync |
| 2 | Pin/unpin works but panel missing, or no real-time sync |
| 1 | Pin works but no indicator or panel |
| 0 | Not implemented |

## Evidence
- Screenshot of pinned message with indicator
- Screenshot of pinned messages panel
