# Order data interface

Expose the following data through the database's native read tools. These names
describe a read interface; tables, collections, or database-native views over the
application's current records are allowed. Do not maintain separate copies for
this interface. Extra columns are allowed.

- `order_account(id, username)` identifies customer accounts. Do not include passwords or tokens in this interface.
- `order_header(id, account_id, total, refunded, status)` contains every order from direct purchase or checkout, including cancelled orders. `total` is the amount booked for the order. `refunded` is the amount refunded so far, initially zero.
- `order_line(id, order_id, item_id, quantity, unit_price)` contains each order's purchased lines and their booked unit prices.
- When carts are available, `order_cart(account_id, item_id, quantity)` contains their current lines. An empty cart has no lines.
- When carts are available, `order_reservation(account_id, item_id, warehouse_id, quantity)` contains any stock held for those carts and already deducted from available `stock.quantity`. If the app does not hold stock for carts, this read interface is empty. This does not require adding stock reservations to the app.
- When warehouse stock is available, `order_allocation(order_line_id, warehouse_id, quantity)` contains the original warehouse quantities used for each order line. Keep these quantities available after cancellation.

Each `id` is a nonempty string or an exact nonnegative integer. Related identifiers
refer to that same `id`; a document may use `_id` when it has no `id`. Item and warehouse
identifiers match the existing `item` and `warehouse` data interfaces and the visible
application actions. Quantities are whole numbers. Money fields use the same currency
units as displayed prices, with at most two decimal places. Status uses the states in
the order interface.

The names and fields above must remain readable by the supplied database credentials
as features are added. Customer screens and writes must use the same underlying records.
This does not require making customer data available to unauthenticated app users.
