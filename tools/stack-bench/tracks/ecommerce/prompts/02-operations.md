# Store operations

Add warehouse operations, order fulfilment, cancellation, returns, and price management.

## New: warehouse staff

- A **staff** account signs in like anyone else but sees the fulfilment area
  instead of the admin area
- Staff accounts are seeded (below); sign-up still creates ordinary customers
- Staff cannot change prices or create items; admins can do everything staff can

## Features

### Fulfilment

Show pending orders in a fulfilment queue, oldest first. Each entry shows its items and
shipping warehouse. Staff can mark an order shipped. Show the status in order history.

Name the shipping write `ship`: server-based stacks expose `POST /api/fulfilment/ship`
with `{ "orderId": ... }`, and SpacetimeDB exposes reducer `shipOrder`.

The
same authentication and staff-only authorization apply through this path.

### Moving stock

An admin can transfer units of an item from one warehouse to another.

### Cancelling and returning

Customers can cancel an order before it ships. Refund the purchase and return its stock
to the supplying warehouse. After shipping, customers can return an item for its purchase
price and the item is restocked. Show cancelled and returned states in order history.

### Prices

An admin can change an item's price. Show prices in the catalog.

### Live operational views

- Low stock: items with 10 units or fewer, most urgent first.
- Warehouse utilisation: total units in each warehouse.
- Category totals: units sold and revenue per category.
- Fulfilment queue depth: number of pending orders.
- Recommended for you: items from categories the customer bought from, most-purchased
  first, excluding items already in their cart. Signed-out visitors see best sellers.

### What must stay true

- An item's stock is always the sum of its warehouse rows, no matter which of the
  actions above last touched it
- Revenue always equals the sum of orders that are still standing — not cancelled,
  minus what was returned
- The best-seller ranking always reflects kept purchases
- Every one of these numbers is the same for every person looking at it

## Starting data

Add to the level 1 catalogue, without disturbing it:

**Categories** — every existing item gets one:

| Category | Items |
|---|---|
| `Home` | Air Purifier, Desk Lamp, Induction Cooktop, Espresso Machine, Coffee Grinder, USB Cable |
| `Audio` | Bluetooth Speaker, Headphones |
| `Computing` | Gaming Mouse, Keyboard, Laptop Stand, Webcam |
| `Photo` | Mirrorless Camera |

**Staff account:** username `staff`, password `stackbench-staff-2026`.

No orders, cancellations, returns or price changes exist at the start.
