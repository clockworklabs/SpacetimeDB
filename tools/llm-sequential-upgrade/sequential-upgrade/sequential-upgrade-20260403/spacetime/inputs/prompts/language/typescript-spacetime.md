# Language: TypeScript + SpacetimeDB

Create this app using **SpacetimeDB as the backend** with **TypeScript**.

## Project Setup

```
apps/chat-app/staging/typescript/<LLM_MODEL>/spacetime/chat-app-YYYYMMDD-HHMMSS/
```

Module name: `chat-app`

## Architecture

**Backend:** SpacetimeDB TypeScript module
**Client:** React + Vite + TypeScript

## Constraints

- Only create/modify code under:
  - `.../backend/spacetimedb/` (server-side TypeScript)
  - `.../client/src/` (client-side TypeScript/React)
- Keep it minimal and readable.

## Branding & Styling

- App title: **"SpacetimeDB Chat"**
- Dark theme using official SpacetimeDB brand colors:
  - Primary: `#4cf490` (SpacetimeDB green)
  - Primary hover: `#4cf490bf` (green 75% opacity)
  - Secondary: `#a880ff` (SpacetimeDB purple)
  - Background: `#0d0d0e` (shade2 — near black)
  - Surface: `#141416` (shade1 — slightly lighter)
  - Border: `#202126` (n6)
  - Text: `#e6e9f0` (n1 — light gray)
  - Text muted: `#6f7987` (n4)
  - Accent: `#02befa` (SpacetimeDB blue)
  - Success: `#4cf490` (green — same as primary)
  - Warning: `#fbdc8e` (SpacetimeDB yellow)
  - Danger: `#ff4c4c` (SpacetimeDB red)
  - Gradient (optional, for headers): `linear-gradient(266deg, #4cf490 0%, #8a38f5 100%)` (green to purple)

## Output

Return only code blocks with file headers for the files you create.
