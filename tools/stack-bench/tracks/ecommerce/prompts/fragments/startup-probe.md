## Catalogue readiness

On server-based stacks (MongoDB/PostgreSQL), expose `GET /api/items`. Return
HTTP 200 and a JSON object with an `items` array. This interface does not define
how the app stores or restores its data. SpacetimeDB apps use the module and do
not need this endpoint.
