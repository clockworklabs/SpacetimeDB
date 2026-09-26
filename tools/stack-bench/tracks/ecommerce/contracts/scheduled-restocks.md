# Scheduled restock application interface

Use `admin-link` to open the administrator area. If these controls are on a separate screen within
it, expose `restocks-link` there to reach them. Use `schedule-restock-item`, `schedule-restock-warehouse`, `schedule-restock-qty`, and
`schedule-restock-delay` for the inputs. Use `schedule-restock-submit` to schedule the restock.
Set its `data-action-input` to a JSON object with exactly `item`, `warehouse`, `quantity`, and
`delaySeconds`. `item` and `warehouse` are their names as strings; `quantity` and
`delaySeconds` are JSON integers. Use `pending-restock-item` for each pending row and set its
`data-entity-id` to the restock's server identifier.
Each row contains the item name and sets `data-quantity` to its integer quantity. Use
`pending-restock-remaining` for its remaining seconds, `pending-restock-cancel` to cancel it,
and `stock-ledger-entry` for a completed stock movement.

<!-- interface:http -->
Expose `POST /api/admin/scheduled-restocks` and `DELETE /api/admin/scheduled-restocks/:id`.
The POST body has the same fields as `data-action-input`.
Write the restock's `data-entity-id` as a decimal number.
<!-- /interface -->

<!-- interface:reducer -->
Expose `schedule_restock` with arguments in this order: `item: string`, `warehouse: string`,
`quantity: u32`, `delaySeconds: u32`; and `cancel_scheduled_restock` with `restockId: u64`.
Write the restock's `data-entity-id` as a decimal number.
<!-- /interface -->

<!-- interface:convex -->
Use `api:schedule_restock` with `{ item, warehouse, quantity, delaySeconds }` and
`api:cancel_scheduled_restock` with `{ restockId }`. The item and warehouse arguments are names.
Use the scheduled-restock document's native `_id` string for `data-entity-id` and `restockId`.
<!-- /interface -->

<!-- interface:supabase -->
Use the `schedule_restock` PostgreSQL function with `{ item, warehouse, quantity, delaySeconds }`
and `cancel_scheduled_restock` with `{ restockId }`. The item and warehouse arguments are names.
Use the identifier accepted by `restockId` for the restock's `data-entity-id`.
<!-- /interface -->
