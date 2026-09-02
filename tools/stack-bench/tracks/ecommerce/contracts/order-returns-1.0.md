# Order return application interface

Use `return-item` inside an `order-item` for each item that can be returned. Do not show it for a
pending order. After a return, the same `order-item` contains the word `returned`.

The existing `orders-toggle`, `order-item`, `item-stock`, `admin-revenue`, and `catalog-link`
interfaces expose the order, stock, and accounting results.
