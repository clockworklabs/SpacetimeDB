# MongoDB

Use MongoDB for the application data. Choose the libraries, architecture, and
project structure.

## Connection

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| Web application | `http://localhost:<VITE_PORT>` |

The MongoDB service is already running as a single-node replica set. Use the exact `DATABASE_URL`. Do not
start another MongoDB server, connect to another instance, or create another
database. Serve the complete application on `<VITE_PORT>`.
Read `DATABASE_URL` from the process environment at startup. It can change between
launches; do not embed it in source or override it with a saved value.
Create `/app/start.sh`. From a clean source checkout, it must install
dependencies, build the complete application, and start it on `<VITE_PORT>`.
The script must not change source files. Leave the application running when the
work is complete.
