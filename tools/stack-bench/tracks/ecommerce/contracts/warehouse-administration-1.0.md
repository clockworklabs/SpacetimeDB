# Warehouse administration application interface

Use `admin-link` to open `admin-panel`. Use `admin-item-row` for each item and `admin-stock` for
its numeric total stock. Use `admin-warehouse-item` for each warehouse. Use `admin-location-row`
for each item and warehouse holding, with `admin-location-qty` for its quantity. Use
`restock-input` and `restock-submit` inside that row. Use `admin-revenue` for numeric total
revenue.

Put a `data-restock-input` attribute on each `admin-location-row`. Its value is a JSON object
with `itemId`, `warehouseId`, and a valid one-unit `quantity`. Identifiers can be JSON numbers or
strings.

The restock action uses the same administrator, stock, and warehouse rules as the visible
application.

## Stock data interface

Other systems read and write stock directly in the database, without calling the application.
Keep singular tables `item(id, name, price)`, `warehouse(id, name)`, and
`stock(item_id, warehouse_id, quantity)` as the source of truth for that surface.

<!-- interface:http -->
Expose `POST /api/admin/restock`.
<!-- /interface -->

<!-- interface:reducer -->
Expose `admin_restock`.
<!-- /interface -->
