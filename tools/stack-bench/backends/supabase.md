# Backend: Supabase

Use a React client with `@supabase/supabase-js`. Keep the database schema and
functions in SQL files under `supabase/migrations/`. The application operations
described below are PostgreSQL functions in the `public` schema. Use the same
functions for the visible controls. Put any Edge Functions in
`supabase/functions/<name>/index.ts`.

## Deployment

The local Supabase service is already running with Auth, the data API (`/rest/v1`),
Realtime, Storage, and Edge Functions. The environment supplies `SUPABASE_URL`,
`SUPABASE_ANON_KEY`, `SUPABASE_SERVICE_ROLE_KEY`, `SUPABASE_DB_URL`,
`VITE_SUPABASE_URL`, and `VITE_SUPABASE_ANON_KEY`. Use these exact values at startup.
Do not embed or save them in source or connect to a hosted project. The web client,
this environment, and Edge Functions all reach it at `SUPABASE_URL`.
Keep `SUPABASE_SERVICE_ROLE_KEY` and `SUPABASE_DB_URL` out of the web client and user
sessions; the service role key bypasses row-level security.

Auth identifies accounts by email address, compared without regard to case. Email
confirmation is off. Passwords must be at least 6 characters and at most 72 bytes. Studio, the management API,
email delivery, and OAuth providers are not available.

Edge Functions are served at `$SUPABASE_URL/functions/v1/<name>` and pick up file
changes on the next request; there is no deploy step. They run on Deno, accept `npm:`
and `jsr:` imports, and receive `SUPABASE_URL`, `SUPABASE_ANON_KEY`,
`SUPABASE_SERVICE_ROLE_KEY`, and `SUPABASE_DB_URL`.

`psql` and Docker are not available. Apply migrations with the `pg` package and
`SUPABASE_DB_URL`, which connects as the `postgres` role.

Create `/app/start.sh`. From a clean source checkout, it must install dependencies,
apply the migrations, build the web application, and start it on `<VITE_PORT>` without
changing source files. Apply migrations and initialize empty data on every start, even
with `APP_WARM_START=1`, so they must be safe to run again; reuse current dependencies
when possible. Keep existing data and accounts during subsequent starts, upgrades, and
repairs. Leave the application running.
