# Bug Report

## Bug 1: Entering a room crashes the app — messages is not iterable

**Feature:** Regression — basic chat broken
**Severity:** Critical

**Steps to reproduce:**
1. Open the app, register a user
2. Click on any room
3. Observed errors:
   - `GET /api/rooms` → 400 Bad Request
   - `GET /api/rooms/4/messages?userId=2` → 500 Internal Server Error
   - `TypeError: messages is not iterable` crash in App.tsx line 444

**Root cause (likely):** The L3 upgrade modified the `/api/rooms` or `/api/rooms/:id/messages` endpoint response format. The client expects an array but is receiving an error object or non-array response. Also the rooms endpoint is returning 400.

**Fix required:**
- Fix `GET /api/rooms` to return 200 with array (check if new query params are required)
- Fix `GET /api/rooms/:id/messages` to return 200 with array
- Ensure `messages` state is always initialized as an array, never null/undefined
- Restart and verify both endpoints return arrays before closing
