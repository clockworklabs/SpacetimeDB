# Warehouse administration application interface

Use `admin-link` to open `admin-panel`. Use `admin-item-row` for each item and `admin-stock` for
its numeric total stock. Use `admin-warehouse-item` for each warehouse. Use `admin-location-row`
for every item in every warehouse, including zero quantities; the row shows the item name and
the warehouse name, with
`admin-location-qty` for its quantity. Use
`restock-input` and `restock-submit` inside that row. Use `admin-revenue` for numeric total
revenue.

Keep all item, warehouse, and holding rows available in the open admin panel, without pagination.

Put a `data-restock-input` attribute on each `admin-location-row`. Its value is a JSON object
with exactly `itemId`, `warehouseId`, and a valid one-unit `quantity`. Identifiers can be JSON numbers or
strings.

Use the same restock action as the visible control.

## Stock data interface

Expose singular native tables or views `item(id, name, price)`, `warehouse(id, name)`, and
`stock(item_id, warehouse_id, quantity)` for direct database access.
`stock.item_id` and `stock.warehouse_id` reference `item.id` and `warehouse.id`; in a document
store they hold the referenced document's `id` value, or its `_id` when it has no `id`.
Each item and warehouse must have a unique ID and name, with one stock holding per item and
warehouse pair. Keep this interface readable with the database's own tools. Native updates to
`stock.quantity` must change the same stock that the application uses. Physical foreign-key
constraints are not required.

<!-- interface:http -->
Expose `POST /api/admin/restock`. The JSON body has the same fields as `data-restock-input`.
<!-- /interface -->

<!-- interface:reducer -->
Expose `admin_restock` with arguments in this order: `itemId: u64`, `warehouseId: u64`,
`quantity`.
<!-- /interface -->

<!-- interface:convex -->
Use `api:admin_restock` with `{ itemId, warehouseId, quantity }`.
The singular native `item` and `warehouse` tables use Convex `_id` identifiers.
The `stock.item_id` and `stock.warehouse_id` fields refer to those same identifiers.
<!-- /interface -->

<!-- interface:supabase -->
Use the `admin_restock` PostgreSQL function with `{ itemId, warehouseId, quantity }`.
The `item`, `warehouse`, and `stock` tables are in the `public` schema.
<!-- /interface -->
