# Ecommerce track — level sequence

A store is a better vehicle than a chat app for the properties that survive a
strong model. Chat's L1 saturated: every backend scored full marks on features,
so the only thing the feature axis measured was how much guidance each backend's
pack contained. A store puts numbers at the centre — stock, quantities, totals,
rankings — and a number is either right or it is not.

The domain is a storefront with a warehouse behind it. Levels are ordered by the
property each one makes verifiable, as in the chat track.

---

## L1 — Storefront and warehouse

**Unlocks:** identity, live derived values, per-account durable state, and
arithmetic that many clients can break at once.

Accounts, a public storefront ranked by sales, buying, a cart, order history,
reviews, and an admin area over warehouses and stock.

Every headline number on this level is **derived from something else and shared
by everyone**: the storefront ranking is a rollup of purchases, an item's stock
is the sum of its rows across warehouses, an item's rating is the average of its
reviews, and revenue is the sum of all orders. Each has to stay correct live, for
signed-out visitors as well as customers. That is one property expressed four
ways, which is what makes the level hard to pass by accident.

The cart carries the other half: it belongs to an account rather than a browser,
so the same customer in two places sees one cart. And purchases are deliberately
**not rate limited**, because the contention suite needs customers to be able to
collide.

L1 is deliberately large — the full anonymous, customer and admin surface — because
a smaller L1 is what saturated on the chat track.

## L2 — Personalisation and catalogue queries

**Unlocks:** per-customer derived state at catalogue scale.

A storefront ranked for *you*: items sharing a category with something you have
bought come first, then the global ranking. Faceted search and filters over a
larger catalogue.

The rule has to be stated precisely enough to be checked arithmetically, which is
why it is a level of its own rather than part of L1.

## L3 — Multi-warehouse operations

**Unlocks:** atomicity across rows.

Transfers between warehouses, and fulfilment that chooses which warehouse serves
an order. A transfer of N units must leave the total unchanged — no unit may be
duplicated or lost, even if the transfer is interrupted or two run at once.

This is the ACID gap stated in inventory terms: it is not one row being
decremented, it is two rows that must move together or not at all.

## L4 — Order lifecycle and deferred work

**Unlocks:** durability of background work.

Orders that move pending to shipped to delivered, carts that expire, restocks
scheduled for later. Verified across a backend restart: work that was pending
must still happen, and expired things must actually be gone rather than hidden.

## L5 — Volume

**Unlocks:** throughput, latency and efficiency under load.

A large catalogue, a long order history and many concurrent shoppers. Ranking and
search latency percentiles, sustained purchases per second, and whether query cost
grows with catalogue size. A flash sale is contention at scale: the L1 oversell
check, but with hundreds of customers.

---

## Scoring axes

Three suites per level, scored separately, for the reason the chat track learned
the hard way: a feature-only score cannot see a cross-cutting property, so an app
can implement everything on the list and still sell one item to two people.

- **features** — the level's functionality
- **invariants** — properties that must hold regardless of feature completeness:
  a purchase needs an account, an order belongs to whoever placed it, the price is
  the store's to set, stock is administered by admins only, the books balance
- **contention** — what many clients doing the same thing at once must leave behind

Contention criteria start at **zero points and stay there** until mutation testing
proves the criterion can catch a real defect. An unproven criterion is a
hypothesis, and this benchmark has already had four hypotheses of that kind
evaporate when a stronger model wrote the competitor's code. They run and report
from the start, so the evidence accumulates; they simply cannot move a score until
they have earned it.
