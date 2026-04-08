# Bug Report

## Bug 1: Reply count displays garbled value instead of integer count

**Feature:** Message Threading

**Description:** The reply count shown on parent messages is incorrect — it appears to be concatenating reply IDs or user identifiers as strings rather than displaying a simple integer count. For example, 3 replies shows as "0111" on one client and "21" on another. Each client shows a different garbled value.

**Root cause to investigate:** The reply count is almost certainly being computed via string concatenation (e.g. `replyCount + newId`) instead of integer arithmetic (e.g. `replyCount + 1`). Check how `replyCount` or equivalent is updated in the frontend when a new reply arrives via WebSocket/SSE. Ensure the count is parsed as an integer before incrementing: `parseInt(count, 10) + 1`.

**Expected:** Reply count shows a plain integer (e.g. "3 replies").
**Actual:** Reply count shows a garbled concatenated string (e.g. "0111", "21") that varies per client.
