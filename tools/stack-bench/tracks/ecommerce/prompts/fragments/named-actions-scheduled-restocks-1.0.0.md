### Named actions for scheduled restocks

These writes already appear in the requested feature. Expose the exact named
actions below through the stack's normal write path.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| schedule a restock | `POST /api/admin/scheduled-restocks` | reducer `scheduleRestock` |
| cancel a scheduled restock | `DELETE /api/admin/scheduled-restocks/:id` | reducer `cancelScheduledRestock` |

The same authentication and authorization rules apply to these calls.
Set `data-action-input` on `schedule-restock-submit` to the current JSON input
for `scheduleRestock`: `item`, `warehouse`, `quantity`, and `delaySeconds`.
