# Backend: Convex

Use a React client and native Convex TypeScript functions. Put the functions and
schema in `convex/`. The application mutations described below are exported from
`convex/api.ts`. Use the same native functions for the visible controls.

## Deployment

The local Convex deployment is already running. The environment supplies
`CONVEX_SELF_HOSTED_URL`, `CONVEX_SELF_HOSTED_ADMIN_KEY`, and `VITE_CONVEX_URL`.
Use these exact values at startup. Do not embed or save them in source or connect
to a hosted project. Keep the deployment key out of the web client and user sessions.
The native HTTP-action origin is `http://127.0.0.1:<EXPRESS_PORT>`.

Create `/app/start.sh`. From a clean source checkout, it must install dependencies,
deploy the functions with `npx convex dev --once --typecheck disable`, build the web
application, and start it on `<VITE_PORT>` without changing source files.
Deploy functions and initialize empty data even with
`APP_WARM_START=1`; reuse current dependencies when possible. Keep existing data
and accounts during subsequent starts, upgrades, and repairs. Leave the application running.
