# Language: Rust + SpacetimeDB

Create this app using **SpacetimeDB as the backend** with **Rust for both the server module and client**.

## Project Setup

```
apps/paint-app/staging/rust/<LLM_MODEL>/spacetime/paint-app-YYYYMMDD-HHMMSS/
```

Module name: `paint-app`

## Architecture

**Backend:** SpacetimeDB Rust module (`spacetimedb` crate)
**Client:** Rust axum server with embedded HTML/CSS/JS GUI that opens in browser

The client should:
- Connect to SpacetimeDB using `spacetimedb-sdk`
- Serve a web GUI via axum on localhost
- Auto-open the browser with the `open` crate
- Use polling or WebSocket for real-time updates from the local axum server

## Constraints

* Only create/modify code under:
    * `.../backend/` (Rust SpacetimeDB module)
    * `.../client/` (Rust client with web-based GUI)
* Keep it minimal and readable.

## Output

Return only code blocks with file headers for the files you create.
