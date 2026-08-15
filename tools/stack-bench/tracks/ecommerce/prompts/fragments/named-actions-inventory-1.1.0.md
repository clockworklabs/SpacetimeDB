### Testing call for restocking

Give the restock write the exact testing call shown here so it can be made
without a browser. Use your stack's ordinary write path; do not add another
transport just for the benchmark.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| restock a warehouse | `POST /api/admin/restock` | reducer `adminRestock` |

The same administrator access, stock, and warehouse rules apply to this call as
to the visible administration interface.
