### Testing calls for pricing and cancellation

Every write below already appears in the requested features. Give it the exact
testing call shown here so the same request can be made without a browser. Use
your stack's ordinary write path; do not add another transport just for the
benchmark.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| change an item's price | `POST /api/admin/price` | reducer `adminChangePrice` |
| cancel an order | `POST /api/orders/:id/cancel` | reducer `cancelOrder` |

The rules do not change because a request arrived this way. Authentication,
staff authorization, price integrity, ownership, and every guarantee above
still apply.
