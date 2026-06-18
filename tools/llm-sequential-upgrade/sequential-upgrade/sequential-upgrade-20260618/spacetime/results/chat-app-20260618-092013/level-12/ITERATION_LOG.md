# Iteration Log

## Iteration 1 — Fix (Build reprompt)

**Category:** Compilation/Build
**What broke:** TypeScript error: `useTable` returns `readonly` arrays; `MessageList` props declared mutable types
**Root cause:** `useTable` from `spacetimedb/react` returns `readonly T[]`; props typed as mutable `T[]`
**What I fixed:** Changed `MessageListProps` fields to `readonly Message[]`, `readonly User[]`, `readonly UserRoomRead[]`
**Files changed:** `client/src/App.tsx` (props interface)
**Redeploy:** Client only

**Server verified:** Client at http://localhost:6173 ✓

## Iteration 2 — Fix (13:XX)

**Category:** Feature Broken
**What broke:** Kicked users still see and receive messages from the kicked room
**Root cause:** Two problems: (1) The message subscription used `tables.message` (all messages globally), so kicked users kept receiving new messages even after their `room_member` row was deleted. (2) The kicked overlay used `position: absolute; rgba(0,0,0,0.7)` which visually overlaid but was semi-transparent, leaving messages visible behind it.
**What I fixed:** (1) Changed the message subscription to a semijoin query: `tables.roomMember.where(m => m.userIdentity.eq(myIdentity)).rightSemijoin(tables.message, ...)` — this filters messages to only rooms where the user is a member at the server/subscription level. When kicked (room_member row deleted), SpacetimeDB automatically removes those messages from the client's local cache. (2) Changed the kicked overlay from a semi-transparent absolute overlay to a conditional render that replaces (not overlays) the messages area and input bar, so messages are not in the DOM at all when kicked. Updated `.kicked-overlay` CSS from `position: absolute` to `flex: 1`.
**Files changed:** `client/src/App.tsx` (subscription query, JSX structure), `client/src/styles.css` (kicked-overlay CSS)
**Redeploy:** Client only

**Server verified:** Client at http://localhost:6173 ✓

## Iteration 3 — Fix (09:XX)

**Category:** Feature Broken
**What broke:** Draft edits from another session (same user, different tab) didn't update the message input in real time; the input only reflected the draft on room selection.
**Root cause:** `messageInput` was only set from the draft inside the `useEffect` that fires on `selectedRoomId` change. After the room was selected, incoming draft updates in `messageDrafts` (subscribed via SpacetimeDB) were never applied to the input.
**What I fixed:** Split the `[selectedRoomId]` effect so only non-input side-effects (scroll, panel resets) stay there. Added a new `useEffect` watching `[messageDrafts, selectedRoomId, myHex]` that finds the current room's draft and calls `setMessageInput` whenever the draft table changes — including updates arriving from other sessions.
**Files changed:** `client/src/App.tsx` (lines ~756-775)
**Redeploy:** Client only

**Server verified:** Client at http://localhost:6173 ✓
