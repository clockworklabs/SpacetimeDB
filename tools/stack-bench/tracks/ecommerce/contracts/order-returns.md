# Order return application interface

Use `return-item` inside an `order-item` for each item that can be returned. After a return,
the same `order-item` contains the word `returned`.

Each ordinary item has an `order-line` containing its name, including while pending.
Set `data-return-input` on that line to JSON with `orderId` and `itemId`, using the
identifiers accepted by the return action.

<!-- interface:http -->
`returnItem` is `POST /api/orders/{orderId}/items/{itemId}/return`.
<!-- /interface -->

<!-- interface:reducer -->
`returnItem` is `return_order_item(orderId, itemId)`.
<!-- /interface -->

The existing `orders-toggle`, `order-item`, `item-stock`, `admin-revenue`, and `catalog-link`
interfaces expose the order, stock, and accounting results.
