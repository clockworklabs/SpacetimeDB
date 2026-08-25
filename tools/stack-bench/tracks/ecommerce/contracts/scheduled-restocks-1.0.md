# Scheduled restock testing interface

Use `schedule-restock-item`, `schedule-restock-warehouse`, `schedule-restock-qty`, and
`schedule-restock-delay` for the inputs. Use `schedule-restock-submit` to schedule the restock.
Set its `data-action-input` to a JSON object with `item`, `warehouse`, `quantity`, and
`delaySeconds`. Use `pending-restock-item` for each pending row,
`pending-restock-remaining` for its remaining seconds, `pending-restock-cancel` to cancel it,
and `stock-ledger-entry` for a completed stock movement.

For direct authorization tests, server-based stacks expose `POST /api/admin/scheduled-restocks`
and `DELETE /api/admin/scheduled-restocks/:id`. SpacetimeDB exposes `scheduleRestock` and
`cancelScheduledRestock`. These calls use the same authorization rules as the application.
