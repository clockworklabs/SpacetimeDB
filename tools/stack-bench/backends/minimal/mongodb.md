# Backend: MongoDB

Use **MongoDB** as the database. Everything else — server framework, how the
browser gets live updates, whether you use an ORM, project layout — is your
choice. Build it the way you think it should be built.

## What the harness needs

| Setting | Value |
|---|---|
| `DATABASE_URL` | `<DATABASE_URL>` |
| Client dev server | must be reachable at `http://localhost:<VITE_PORT>` |
| API server, if you run one | port `<EXPRESS_PORT>` |

Use this exact `DATABASE_URL`. Do not point at another MongoDB instance and do
not create databases outside it. The client must be served on that port and left
running when you are done.

## Branding & Styling

- App title: **"MongoDB <APP_NOUN>"**
- Dark theme using official MongoDB brand colors:
  - Primary: `#00ED64` (MongoDB green)
  - Primary hover: `#00C957` (darker green)
  - Secondary: `#00684A` (MongoDB forest green)
  - Background: `#001E2B` (MongoDB dark slate)
  - Surface: `#023430` (deep green-slate)
  - Border: `#1C2D38` (muted slate border)
  - Text: `#E8EDEB` (light gray)
  - Text muted: `#889397` (MongoDB gray)
  - Accent: `#00ED64` (MongoDB green)
  - Success: `#00ED64` (green for online indicators)
  - Warning: `#FFC010` (MongoDB amber)
  - Danger: `#FF4F4F` (MongoDB red)
