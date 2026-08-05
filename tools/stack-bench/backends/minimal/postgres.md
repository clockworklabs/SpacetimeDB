# Backend: PostgreSQL

Use **PostgreSQL** as the database. Everything else — server framework, how the
browser gets live updates, whether you use an ORM, project layout — is your
choice. Build it the way you think it should be built.

## What the harness needs

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| Client dev server | must be reachable at `http://localhost:<VITE_PORT>` |
| API server, if you run one | port `<EXPRESS_PORT>` |

Use this exact `DATABASE_URL`. Do not point at another PostgreSQL instance and do
not create databases outside it. The client must be served on that port and left
running when you are done.

## Branding & Styling

- App title: **"PostgreSQL Chat"**
- Dark theme using official PostgreSQL brand colors:
  - Primary: `#336791` (PostgreSQL blue)
  - Primary hover: `#008bb9` (lighter PostgreSQL blue)
  - Secondary: `#0064a5` (dark PostgreSQL blue)
  - Background: `#1a1a2e` (dark navy)
  - Surface: `#16213e` (slightly lighter)
  - Border: `#2a2a4a` (muted border)
  - Text: `#e8e8e8` (light gray)
  - Text muted: `#848484` (PostgreSQL light grey)
  - Accent: `#008bb9` (PostgreSQL light blue)
  - Success: `#27ae60` (green for online indicators)
  - Warning: `#f26522` (PostgreSQL light orange)
  - Danger: `#cc3b03` (PostgreSQL dark orange/red)
