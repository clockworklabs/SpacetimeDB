# Supabase

Use Supabase for application data, accounts, and backend functions. Choose the client
libraries, architecture, and project structure.

## Connection

The local Supabase service is already running with Auth, the data API (`/rest/v1`),
Realtime, Storage, and Edge Functions. The browser, this environment, and Edge Functions
all reach it at `SUPABASE_URL`. The web client uses `VITE_SUPABASE_URL` and
`VITE_SUPABASE_ANON_KEY`; `SUPABASE_ANON_KEY` holds the same public key.
`SUPABASE_SERVICE_ROLE_KEY` bypasses row-level security, and `SUPABASE_DB_URL` connects
to PostgreSQL as the `postgres` role. Keep both on the server; they are not end-user
credentials. These values are supplied in the process environment and can change
between launches. Do not embed or save them in source.

Use this service. Do not start another Supabase server or connect to a hosted project.
Studio, the management API, email delivery, and OAuth providers are not available.
Auth identifies accounts by email address, compared without regard to case. Email
confirmation is off. Passwords must be at least 6 characters and at most 72 bytes.

Edge Functions in `supabase/functions/<name>/index.ts` are served at
`$SUPABASE_URL/functions/v1/<name>` and pick up file changes on the next request. They
run on Deno, accept `npm:` and `jsr:` imports, and receive `SUPABASE_URL`,
`SUPABASE_ANON_KEY`, `SUPABASE_SERVICE_ROLE_KEY`, and `SUPABASE_DB_URL`.
`psql` and Docker are not available; run SQL from Node through `SUPABASE_DB_URL`.
Serve the complete application on `<VITE_PORT>`.

Create `/app/start.sh`. From a clean source checkout, it must install dependencies,
apply the database schema, build the web application, and start the complete application.
The script must not change source files. Apply the schema and initialize empty application
data on every start, even when `APP_WARM_START=1`; that flag only permits reusing current
dependencies. Preserve existing application data and accounts. Leave the application
running when work is complete.
