# Bug Report

## Bug 1: Room member list does not update in real-time (STILL NOT FIXED)

**Feature:** Real-Time Permissions / Basic Chat

**Description:** This bug was attempted to be fixed but the problem persists. The room member list is NOT subscribing to live updates from the server. It only reflects the state at the time the user entered the room.

**Root cause to investigate:** The frontend is likely fetching room members once on room join (e.g. a single HTTP GET or one-time query) rather than subscribing to a real-time channel or polling for changes. The fix must ensure that when any user joins or leaves a room, ALL currently connected members in that room see the updated member list immediately without any navigation required.

**Specific failure cases:**
- A joins room, B joins room → A still only sees themselves in the member list
- B leaves room → A still sees B in the member list
- Only after A navigates away and back does the list update

**Required fix:** Use WebSocket/SSE push or polling (e.g. every 2-3 seconds) to keep the member list live. The member list must reflect reality within a few seconds without any user action.
