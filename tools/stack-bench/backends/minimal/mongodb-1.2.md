# MongoDB

Use MongoDB for the application data. Choose the libraries, architecture, and
project structure.

## Connection

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| Web application | `http://localhost:<VITE_PORT>` |
| Application service port | `<EXPRESS_PORT>` |

Use the exact `DATABASE_URL`. Do not connect to another MongoDB instance or
create another database. Serve the web application on the stated port and leave
it running when the work is complete.
