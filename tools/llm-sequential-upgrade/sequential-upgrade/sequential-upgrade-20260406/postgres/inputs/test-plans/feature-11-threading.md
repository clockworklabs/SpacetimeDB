# Feature 11: Message Threading

**Max Score: 3** | **Multi-user: Single tab OK, multi for sync**

## Preconditions
- At least one message exists in the room to reply to

## Test Steps

### Step 1: Reply to Message
1. **Tab A**: Find a "reply" button on an existing message. Use `find("reply")` or look for a reply icon when hovering over a message.
2. Click reply.
3. **Verify**: A compose UI appears that's contextually linked to the parent message (reply box, thread view, or inline reply).
4. Type "This is a thread reply" and send.

**Criterion:** Users can reply to specific messages, creating a thread (1 point)

### Step 2: Reply Count
1. **Tab A**: Check the parent message for a reply count indicator. Use `get_page_text` and search for "1 reply" or "replies" or a thread icon with count.

**Criterion:** Parent messages show reply count and preview (0.5 points)

### Step 3: Thread View
1. Click on the parent message or the reply count to open the threaded view.
2. **Verify**: The thread view shows the parent message and the reply "This is a thread reply".
3. Send another reply in the thread: "Second reply".
4. **Verify**: Both replies are visible in the thread.

**Criterion:** Threaded view shows all replies to a message (1 point)

### Step 4: Real-Time Thread Sync
1. **Tab B**: Navigate to the same thread (click on the parent message).
2. **Tab A**: Send another reply in the thread: "Third reply from Alice".
3. **Tab B**: Verify the new reply appears in real-time without refresh.

**Criterion:** New replies sync in real-time to thread viewers (0.5 points)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Threading works but no reply count or preview |
| 1 | Can reply but threaded view is broken |
| 0 | Not implemented |

## Evidence
- Screenshot of thread view with replies
- Screenshot of parent message showing reply count
