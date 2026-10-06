# Convex

Use Convex for application data and backend functions. Choose the client libraries,
architecture, and project structure.

## Connection

The local Convex service is already running. Read its URL from
`CONVEX_SELF_HOSTED_URL` and its deployment key from `CONVEX_SELF_HOSTED_ADMIN_KEY`.
The web client uses `VITE_CONVEX_URL`. These values are supplied in the process
environment and can change between launches. Do not embed or save them in source.
Keep the deployment key on the server; it is not an end-user credential.

Use this deployment. Do not start another Convex server or connect to a hosted project.
The native HTTP-action origin is `http://127.0.0.1:<EXPRESS_PORT>`.
Serve the complete application on `<VITE_PORT>`.

Create `/app/start.sh`. From a clean source checkout, it must install dependencies,
deploy the Convex functions, build the web application, and start the complete application.
The script must not change source files. Deploy the functions and initialize empty application data even when
`APP_WARM_START=1`; that flag only permits reusing current dependencies. Preserve
existing application data and accounts. Leave the application running when work is complete.
