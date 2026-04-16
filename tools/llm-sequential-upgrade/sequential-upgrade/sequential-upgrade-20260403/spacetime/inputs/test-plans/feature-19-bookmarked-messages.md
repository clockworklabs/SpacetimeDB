# Feature 19: Bookmarked/Saved Messages

**Max Score: 3** | **Multi-user: No (single user, personal feature)**

## Preconditions
- User (Alice in Tab A) is registered and in a room with messages

## Test Steps

### Step 1: Bookmark a Message
1. **Tab A (Alice)**: Hover over a message. Look for a bookmark/save icon.
2. Click the bookmark icon.
3. **Verify**: Visual feedback that the message is bookmarked (filled icon, toast, etc.).

**Criterion:** Users can bookmark messages (1 point)

### Step 2: Saved Messages Panel
1. **Tab A (Alice)**: Look for a "Saved" or bookmark icon in the sidebar.
2. Click it to open the saved messages panel.
3. **Verify**: The bookmarked message appears with content, sender, channel name, and timestamp.

**Criterion:** Saved messages panel shows bookmarks with context (1 point)

### Step 3: Remove Bookmark and Privacy
1. **Tab A (Alice)**: Remove the bookmark (via the message or the panel).
2. **Verify**: Message disappears from saved panel.
3. **Tab B (Bob)**: Open the saved messages panel.
4. **Verify**: Bob's saved list is empty — bookmarks are personal.

**Criterion:** Remove works, bookmarks are private per-user (1 point)

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria pass |
| 2 | Bookmark and panel work but missing remove or privacy |
| 1 | Can bookmark but no panel |
| 0 | Not implemented |
