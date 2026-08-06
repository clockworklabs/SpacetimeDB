# Level 2 — Running the business

The store works. Now it has to be **operated**: stock moved between warehouses,
orders fulfilled, prices changed, orders cancelled and returned — while everyone
watching sees the consequences immediately.

Everything below is ordinary retail work. What makes it demanding is that each
action touches several different views of the same data, and every one of those
views belongs to someone who is looking at it right now.

## The people watching

Four kinds of viewer, each seeing a different projection of the same store:

- **A visitor**, signed out — the storefront
- **A customer** — their cart, their orders, their recommendations
- **Warehouse staff** — the fulfilment queue and where stock physically is
- **An administrator** — every item, every warehouse, the money

A change made by any of them must reach all of the others it affects, without a
reload, every time. There is no acceptable action that updates some views and not
others.

## New: warehouse staff

- A **staff** account signs in like anyone else but sees the fulfilment area
  instead of the admin area
- Staff accounts are seeded (below); sign-up still creates ordinary customers
- Staff cannot change prices or create items; admins can do everything staff can

## Features

### Fulfilment

- Every order is **pending** when placed, and appears in the **fulfilment queue**
  in the order it was placed
- Staff **mark an order shipped**. It leaves the queue for everyone watching the
  queue, and the customer's order history shows it as shipped, live.
- The queue shows, for each order, its items and which warehouse each will ship
  from — the warehouse holding stock for that item, chosen the same way a purchase
  drains it

### Moving stock

- An admin can **transfer** a number of units of an item from one warehouse to
  another
- A transfer **moves stock, it does not create or destroy it**: the item's total
  is identical before and after, and the two warehouses' numbers change together.
  A transfer that would leave a warehouse short is refused and changes nothing.
- Transfers are visible immediately: the storefront total is unchanged, but the
  per-warehouse numbers staff and admins see both move at once

### Cancelling and returning

- A customer can **cancel** an order that has not shipped. The stock goes back to
  the warehouse it came from, the order leaves the fulfilment queue, and revenue
  falls by that order's total.
- A customer can **return** an item from an order that has shipped. The stock comes
  back, revenue falls by what was paid for it, and the order shows the item as
  returned.
- A cancelled or returned order's items **stop counting as purchases** — the
  best-seller ranking reflects what was actually kept

### Prices

- An admin can **change an item's price**
- The storefront shows the new price immediately, to everyone
- **Past orders keep the price that was paid.** Changing a price never alters the
  history, the revenue already recorded, or a customer's receipt.
- An item in someone's cart at the moment the price changes is charged the **new**
  price at checkout, and the cart total they are looking at updates to match

### Live operational views

These exist so the people running the store can see its state. Each is derived
from the same data as everything else, and each must be correct the instant any
action above changes it:

- **Low stock** — an admin list of every item whose total is at or below 10 units,
  most urgent first. Items enter and leave this list as stock moves, sells, is
  restocked, cancelled or returned.
- **Warehouse utilisation** — for each warehouse, the total units it holds, live
- **Category totals** — every item belongs to a category (below); for each
  category, how many units have been sold and the revenue they earned, live
- **Fulfilment queue depth** — how many orders are waiting, visible to staff and
  admins, live
- **Recommended for you** — for a signed-in customer, the items from categories
  they have bought from, most-purchased first, excluding items already in their
  cart. For a signed-out visitor this is simply the best sellers.

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
| `Home` | Air Purifier, Desk Lamp, Induction Cooktop, Espresso Machine, Coffee Grinder |
| `Audio` | Bluetooth Speaker, Headphones |
| `Computing` | Gaming Mouse, Keyboard, Laptop Stand, Webcam |
| `Photo` | Mirrorless Camera |

**Staff account:** username `staff`, password `stackbench-staff-2026`.

No orders, cancellations, returns or price changes exist at the start.
