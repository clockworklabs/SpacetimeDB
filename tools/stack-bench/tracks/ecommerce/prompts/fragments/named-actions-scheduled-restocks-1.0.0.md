### Testing calls for scheduled restocks

These writes already appear in the requested feature. Give them the exact
testing calls below so Stack Bench can verify server authorization without a
browser. Use the stack's normal write path.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| schedule a restock | `POST /api/admin/scheduled-restocks` | reducer `scheduleRestock` |
| cancel a scheduled restock | `DELETE /api/admin/scheduled-restocks/:id` | reducer `cancelScheduledRestock` |

The same authentication and authorization rules apply to these calls.
Set `data-action-input` on `schedule-restock-submit` to the current JSON input
for `scheduleRestock`: `item`, `warehouse`, `quantity`, and `delaySeconds`.
