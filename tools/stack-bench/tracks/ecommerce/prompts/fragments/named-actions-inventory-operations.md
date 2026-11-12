### Named action for stock transfers

The transfer write already appears in the requested features. Give it the exact
named action shown here so the same request can be made without a browser. Use
your stack's ordinary write path; do not add another transport for this action.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| transfer stock between warehouses | `POST /api/admin/transfer` | reducer `adminTransferStock` |

Use the same action as the visible transfer control.
