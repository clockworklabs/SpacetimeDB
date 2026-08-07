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

## Systems invariants (both tracks) — the failures that never appear in a demo

Feature tests measure what an app does when one person uses it politely. The
systems suite (`scenarios/01-systems.json`, both tracks) measures what survives
the conditions production actually has. Classes, and where each stands:

**Out-of-band writes** (implemented, withheld): another system writes to the
database — a cron job, an ERP sync, moderation tooling. The spec requires a
`scripts/backoffice.mjs` that writes to the database directly, and criteria
901a/b assert open clients reflect the change live. A broadcast layer only
announces writes that went through it; a subscription to the data itself does
not care who wrote.

**Enumeration during mutation** (implemented, withheld): a list is fetched while
its contents change. Criterion 902a asserts the moved row lands exactly once —
fetch-then-merge architectures show it zero times (stale) or twice (fetch plus
its own live echo).

**Multiple app servers** (designed, not yet implemented): two instances of the
app's server against the same database, actors split between them — the classic
socket.io failure, where a message sent through server A never reaches clients
on server B. Needs: the prompt to require the server to honour a PORT override,
bench.mjs to launch a second instance, and the grader to take per-actor URLs.
SpacetimeDB has no app-server tier to duplicate, which is the point being
measured.

**Fine-grained subscription lifecycles** (L2 material): profiles and friend
lists that a client subscribes to when a panel opens and drops when it closes.
Fetch-on-open architectures go stale the moment the panel outlives its fetch.
Belongs with the L2 feature specs, where the surfaces exist.

**Races and transactions** (partially covered today): oversell and double-spend
live in the contention suite; stock conservation is invariant 104. Missing is
cross-row atomicity under interleaving — a warehouse transfer racing a purchase,
where a non-transactional backend can lose or duplicate a unit between rows.
Belongs in 02-operations, where transfers exist.

**Extensibility** (measured by the ladder itself): the cost of L2 on top of L1 —
turns, dollars, and whether L1 criteria still pass after the upgrade — is the
extensibility number. No new criteria needed; the protocol is to re-run the L1
suites after every upgrade and report regressions as first-class results.

The promotion rule is the same as contention's, with no exceptions: every
systems criterion starts at zero points and earns them only when it
demonstrably fails a real build AND mutation testing shows it catches the
defect it claims to catch.

**Who tests the back-office test.** The app authors the lever AND the surface
being judged, so the criterion is anchored three ways or it is not a test:

1. *External arithmetic* — expected values derive from the dictated seed
   (Desk Lamp 55+45; set East to 5 ⇒ the UI must read exactly 50). The app
   owns the lever, never the answer key. A no-op or wrong-field script cannot
   match a number it does not control.
2. *Server-down execution* — stop the app server, run the script, restart,
   the value must have persisted. Kills scripts that call the app's API and
   writes that only patched server memory. Requires the stopAppServer /
   startAppServer step verbs; NOT YET IMPLEMENTED, and 901 must not be
   promoted before it is.
3. *Counterfeit levers* — promotion requires harness-authored mutant scripts
   (no-op, API-calling, memory-patch) each swapped into a passing reference
   app and CAUGHT.

**Why the criterion is fair**, stated before a hostile reviewer states it for
us: every backend has a legitimate, native path to passing. Postgres apps can
pass with triggers plus LISTEN/NOTIFY; mongo apps can pass with change
streams — both real, documented, production-normal features. SpacetimeDB
passes with no additional work because no write path exists outside its sync
layer: reducers and SQL both flow through the commit stream subscriptions
read from. That means the script's write is technically "in-band" on
SpacetimeDB — which is the property under test, not a loophole: the
measurement is what it COSTS each stack to make external writes safe, and
"nothing, by construction" is a finding, not a rig. The criterion is winnable
by an expert human on all three stacks; the difference is the bill.

Residual, stated so nobody over-claims: a file-outbox design (script writes a
queue, server ingests) passes all three anchors, while a genuinely foreign
system writing rows directly would bypass it. Schema-blind grading cannot
close that. The property this criterion may honestly claim is "has a
documented external-edit path that is live and survives its server being
down" — which a pure broadcast architecture does not have — and results must
be published under that claim, not the stronger one.
