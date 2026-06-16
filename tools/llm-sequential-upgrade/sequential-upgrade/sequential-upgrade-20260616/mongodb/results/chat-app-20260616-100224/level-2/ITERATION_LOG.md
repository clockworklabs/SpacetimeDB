## Iteration 1 — Fix (10:15)

**Category:** Feature Broken
**What broke:** Online presence marks a user offline when one of their two browser tabs closes, even though the other tab is still connected.
**Root cause:** The `disconnect` handler unconditionally called `User.findOneAndUpdate({ name: user }, { online: false })` without checking whether the user had other active sockets. The `socketId` field in the DB only stored the most recent socket, so there was no way to know about earlier connections.
**What I fixed:** Added an in-memory `userSockets` map (`userName -> Set<socketId>`). On `authenticate`, the new socket ID is added to the set. On `disconnect`, the socket ID is removed; the user is only marked offline in MongoDB when their set becomes empty.
**Files changed:** server/src/index.ts (lines 27-29, 184-193, 226-244)
**Redeploy:** Server only

**Server verified:** Client at http://localhost:6373 ✓
