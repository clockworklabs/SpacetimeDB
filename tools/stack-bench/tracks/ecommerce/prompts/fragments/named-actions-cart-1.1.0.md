### Testing calls for purchases and carts

Every write below already appears in the requested features. Give it the exact
testing call shown here so the same request can be made without a browser. Use
your stack's ordinary write path; do not add another transport just for the
benchmark.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| buy one unit of an item | `POST /api/items/:id/buy` | reducer `buyNow` |
| add an item to the cart | `POST /api/cart` | reducer `addToCart` |
| check out the cart | `POST /api/checkout` | reducer `checkout` |

The rules do not change because a request arrived this way. Authentication,
stock limits, price integrity, and every guarantee above still apply.
