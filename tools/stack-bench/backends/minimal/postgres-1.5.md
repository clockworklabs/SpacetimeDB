# PostgreSQL

Use PostgreSQL for the application data. Choose the libraries, architecture,
and project structure.

## Connection

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| Web application | `http://localhost:<VITE_PORT>` |

The PostgreSQL service is already running. Use the exact `DATABASE_URL`. Do not
start another PostgreSQL server, connect to another instance, or create another
database. Serve the complete application on `<VITE_PORT>`.
Create `/app/start.sh`. From a clean source checkout, it must install
dependencies, build the complete application, and start it on `<VITE_PORT>`.
The script must not change source files. Leave the application running when the
work is complete.
