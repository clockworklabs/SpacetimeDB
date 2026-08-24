## Catalogue readiness

On server-based stacks (MongoDB/PostgreSQL), expose `GET /api/items`. Return
HTTP 200 and a JSON object with an `items` array. The test runner uses this
endpoint to know when the catalogue is ready. This interface does not define
how the app stores or restores its data. SpacetimeDB apps are checked through
the module and do not need this endpoint.
