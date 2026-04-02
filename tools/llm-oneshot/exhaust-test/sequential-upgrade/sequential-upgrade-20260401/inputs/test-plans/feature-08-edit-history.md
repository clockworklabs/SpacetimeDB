# Feature 8: Message Editing with History

**Max Score: 3** | **Multi-user: Single tab OK for edit, multi for sync**

## Preconditions
- User has sent at least one message that can be edited

## Test Steps

### Step 1: Edit Message
1. **Tab A**: Find a message Alice sent. Look for an edit option — could be an edit icon, pencil button, or right-click/long-press menu. Use `find("edit")` on or near Alice's message.
2. Click edit, change the text to "Edited message content".
3. Submit the edit.
4. **Verify Tab A**: Message text updates to "Edited message content".

**Criterion:** Users can edit their own messages (1 point)

### Step 2: Edited Indicator
1. **Tab A**: Look for "(edited)" text near the edited message. Use `get_page_text` and search for "edited".

**Criterion:** "(edited)" indicator shows on edited messages (0.5 points)

### Step 3: Edit History
1. Click on the "(edited)" indicator or look for a "history" button. Use `find("history")` or click on "(edited)".
2. **Verify**: A view showing the original message text and the edited version appears.
3. Use `get_page_text` to confirm both versions are displayed.

**Criterion:** Edit history is viewable by other users (1 point)

### Step 4: Real-Time Sync
1. **Switch to Tab B**: Verify the edited message shows the new content "Edited message content" WITHOUT refreshing.
2. Also verify "(edited)" indicator is visible in Tab B.

**Criterion:** Edits sync in real-time to all viewers (0.5 points)

### Step 5: Ownership Check
1. **Tab B**: Try to edit Alice's message. The edit option should NOT be available for other users' messages.

## Scoring

| Score | Criteria Met |
|-------|-------------|
| 3 | All criteria met |
| 2 | Editing works but history not viewable or no "(edited)" label |
| 1 | Can edit but changes don't sync in real-time |
| 0 | Not implemented |

## Evidence
- Screenshot showing "(edited)" indicator
- Screenshot of edit history view
