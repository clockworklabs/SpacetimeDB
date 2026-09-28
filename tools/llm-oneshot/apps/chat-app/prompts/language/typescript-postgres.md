# Language: TypeScript + PostgreSQL

Create this app using **PostgreSQL as the backend** with **TypeScript**.

## Project Setup

```
apps/chat-app/staging/typescript/<LLM_MODEL>/postgres/chat-app-YYYYMMDD-HHMMSS/
```

Database name: `chat-app`

## Architecture

**Backend:** Node.js + Express + Drizzle ORM + Socket.io
**Client:** React + Vite + TypeScript

## Constraints

- Only create/modify code under:
  - `.../server/` (server-side TypeScript)
  - `.../client/` (client-side TypeScript/React)
- Keep it minimal and readable.

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

## Output

Return only code blocks with file headers for the files you create.
