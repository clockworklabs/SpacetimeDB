# Iteration Log

## Iteration 1 — Fix (Build reprompt)

**Category:** Compilation/Build
**What broke:** TypeScript error: `useTable` returns `readonly` arrays; `MessageList` props declared mutable types
**Root cause:** `useTable` from `spacetimedb/react` returns `readonly T[]`; props typed as mutable `T[]`
**What I fixed:** Changed `MessageListProps` fields to `readonly Message[]`, `readonly User[]`, `readonly UserRoomRead[]`
**Files changed:** `client/src/App.tsx` (props interface)
**Redeploy:** Client only

**Server verified:** Client at http://localhost:6173 ✓
