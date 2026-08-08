# Level 3 — Work that happens later

The store is operated. Now it has to keep promises made about the **future**:
holds that expire, restocks that arrive on a schedule, orders that advance on
their own, carts that go stale. Nobody is watching when these happen — and they
must happen anyway.

This is the level where "the server was restarted" stops being an excuse.
Deferred work that a running process was holding in memory is work that was
never really scheduled.

## Reservations that expire

- Adding an item to the cart takes a **reservation** on one unit for **90
  seconds**: storefront stock drops immediately, for everyone
- If the customer checks out within the window, the reservation becomes a sale
- If they do not, the reservation **expires on its own** and the unit returns to
  storefront stock — visibly, without anyone reloading or clicking
- A cart line whose reservation expired is marked expired, not silently removed
- Extending is allowed: touching the quantity renews the 90 seconds

## Scheduled restocks

- An admin can schedule a restock: item, warehouse, quantity, and a delay in
  seconds
- The **pending restocks** list shows each one with its remaining time, live
- When the delay elapses the restock applies itself — stock rises for every
  viewer, the entry leaves the pending list, and it appears in the stock ledger
  with the time it actually ran
- An admin can cancel a pending restock before it fires; cancelled restocks
  never apply

## Orders that advance themselves

- An order placed in `pending` moves to `shipped` **60 seconds** after it is
  fulfilled by staff, without anyone acting
- A shipped order moves to `delivered` **60 seconds** after that
- Each transition is visible live in the customer's order history and in the
  staff queue
- A cancelled order stops advancing — cancellation wins over any pending
  transition, whenever it arrives

## Abandoned carts

- A cart untouched for **5 minutes** is abandoned: its reservations are released
  and the customer is told the cart expired
- An abandoned cart is empty, not hidden — a customer who returns sees an empty
  cart, and the stock is back on the shelf

## What must stay true

- **Deferred work survives a restart.** Everything above must still happen if
  the backend restarts between the scheduling and the firing. Work scheduled
  before a restart fires after it, once, at the right time.
- **Exactly once.** A restart, a reconnect, or two servers must never double-apply
  a scheduled restock or advance an order twice. If it fired, it fired once.
- **Nothing fires early.** A restart must not cause pending work to run
  immediately just because it is being re-read.
- **The clock is the database's, not the browser's.** Countdowns shown in the UI
  may be rendered client-side, but what actually expires is decided by the
  server: a client with a wrong clock, or no client at all, changes nothing.
- **Stock conservation still holds** across reservations, expiries, scheduled
  restocks and cancellations: units are never created or destroyed by the
  passage of time.

## Starting data

Everything from level 1 and level 2, unchanged. No pending restocks, no
reservations and no abandoned carts exist at the start.
