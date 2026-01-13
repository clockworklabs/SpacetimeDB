# Language: Rust + SpacetimeDB

Create this app using **SpacetimeDB as the backend** with **Rust for both server module and client**.

## Project Setup

```
apps/chat-app/staging/rust/<LLM_MODEL>/spacetime/chat-app-YYYYMMDD-HHMMSS/
```

Module name: `chat-app`

## Architecture

**Backend:** SpacetimeDB Rust module (`spacetimedb` crate)
**Client:** Rust CLI application using `spacetimedb-sdk`

The client should:
- Connect to SpacetimeDB using `spacetimedb-sdk`
- Provide a terminal-based interface (TUI with ratatui/crossterm, or simple stdin/stdout)
- Display real-time updates in the terminal
- Show clear command structure and help

## Constraints

* Only create/modify code under:
    * `.../backend/` (Rust SpacetimeDB module)
    * `.../client/` (Rust CLI application)
* Keep it minimal and readable.

## Output

Return only code blocks with file headers for the files you create.
