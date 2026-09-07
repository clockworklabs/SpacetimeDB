# Scheduled restock application interface

Use `admin-link` to open the administrator area. If these controls are on a separate screen within
it, expose `restocks-link` there to reach them. Use `schedule-restock-item`, `schedule-restock-warehouse`, `schedule-restock-qty`, and
`schedule-restock-delay` for the inputs. Use `schedule-restock-submit` to schedule the restock.
Set its `data-action-input` to a JSON object with exactly `item`, `warehouse`, `quantity`, and
`delaySeconds`. Use `pending-restock-item` for each pending row and set its
`data-entity-id` to the restock's server identifier, written as a decimal number. Use
`pending-restock-remaining` for its remaining seconds, `pending-restock-cancel` to cancel it,
and `stock-ledger-entry` for a completed stock movement.

<!-- interface:http -->
Expose `POST /api/admin/scheduled-restocks` and `DELETE /api/admin/scheduled-restocks/:id`.
The POST body has the same fields as `data-action-input`.
<!-- /interface -->

<!-- interface:reducer -->
Expose `schedule_restock` with arguments in this order: `item`, `warehouse`, `quantity`,
`delaySeconds`; and `cancel_scheduled_restock` with `restockId: u64`.
<!-- /interface -->
