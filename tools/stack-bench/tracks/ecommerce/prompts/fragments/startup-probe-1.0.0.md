## Startup data

The server seeds the starting data at startup whenever it is missing. A reset
or restored database must come back as a working store with the full
catalogue, not an empty one; seeding only on first install is not enough.

On server-based stacks (MongoDB/PostgreSQL), expose `GET /api/items` returning
HTTP 200 with a JSON object whose `items` key is an array of the catalogue
items. The harness reads this endpoint to confirm the server is up and its
starting data is seeded before grading begins; an app whose catalogue lives at
a different path or returns a bare array cannot be verified as ready.
SpacetimeDB apps are checked through the module instead and need no such
endpoint.
