# Bug Report

## Bug 1: DM room disappears from sidebar when the other user goes offline

**Feature:** Direct Messages

**Description:** When a DM exists between two users and one of them goes offline, the DM room vanishes from the other user's sidebar immediately. It only reappears when the page is refreshed or the other user rejoins.

**Root cause to investigate:** The sidebar is almost certainly filtering DM rooms by checking if the other participant is in the current online users list. When they disconnect and are removed from that list, the DM room is filtered out. DM rooms must be rendered from a persistent rooms list (fetched from the DB), NOT from the live online users state.

**Steps to reproduce:**
1. a1 joins
2. c3 joins
3. c3 DMs a1
4. a1 sees "@c3" DM in sidebar ✅
5. c3 leaves → a1's "@c3" DM immediately disappears from sidebar ❌
6. Only returns after a1 refreshes or c3 rejoins

**Expected:** DM rooms persist in the sidebar regardless of whether the other participant is currently online. The DM room is a persistent room stored in the database, not a transient online-only construct.
**Actual:** DM room disappears from sidebar when the other participant goes offline.
