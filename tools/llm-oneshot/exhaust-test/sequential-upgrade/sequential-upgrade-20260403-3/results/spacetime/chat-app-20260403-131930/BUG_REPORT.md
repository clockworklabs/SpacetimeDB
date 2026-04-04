# Bug Report

## Bug 1: Message drafts not persisted — lost on room switch or page refresh

**Feature:** Draft Sync

**Description:** Typing a message and switching to another room does not save the draft. Switching back to the original room shows an empty input. Similarly, refreshing the page does not restore any draft text.

**Expected:**
- Typing in a room auto-saves the draft (no button needed)
- Switching rooms and back restores the draft text in the input
- Refreshing the page restores the draft text
- Each room has its own independent draft

**Actual:** Draft is lost immediately on room switch or page refresh. No draft persistence is implemented.
