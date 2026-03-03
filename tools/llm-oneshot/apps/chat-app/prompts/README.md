# Chat App Prompt System

Modular prompts for testing SpacetimeDB vs PostgreSQL with varying feature sets.

## Structure

```
prompts/
├── README.md
├── features/               # 15 feature building blocks
│   ├── 01_basic.md
│   ├── 02_typing_indicators.md
│   └── ...
├── composed/               # Pre-built cumulative prompts (language-agnostic)
│   ├── 01_basic.md
│   ├── 02_scheduled.md
│   ├── ...
│   └── 12_full.md
├── language/               # Language/backend-specific setup (small files)
│   ├── typescript-spacetime.md
│   ├── typescript-postgres.md
│   ├── rust-spacetime.md
│   └── csharp-spacetime.md
├── grading_rubric.md
└── grading_checklist.md
```

## How to Use

**Combine a language file + a composed prompt:**

1. Pick a language: `language/rust-spacetime.md`
2. Pick a feature level: `composed/12_full.md`
3. Concatenate them (language file first)

Example:

```
@language/typescript-spacetime.md @composed/12_full.md execute
```

## Feature Levels

Each level is **cumulative** — includes all previous features.

| Level | Name          | New Features Added                        |
| ----- | ------------- | ----------------------------------------- |
| 01    | basic         | Basic Chat, Typing, Read Receipts, Unread |
| 02    | scheduled     | + Scheduled Messages                      |
| 03    | realtime      | + Ephemeral Messages                      |
| 04    | reactions     | + Message Reactions                       |
| 05    | edit_history  | + Message Editing                         |
| 06    | permissions   | + Real-Time Permissions                   |
| 07    | presence      | + Rich User Presence                      |
| 08    | threading     | + Message Threading                       |
| 09    | private_rooms | + Private Rooms & DMs                     |
| 10    | activity      | + Activity Indicators ⭐                  |
| 11    | drafts        | + Draft Sync ⭐                           |
| 12    | full          | + Anonymous Migration ⭐                  |

⭐ = Features that particularly favor SpacetimeDB

## Language Files

| File                      | Stack                                         |
| ------------------------- | --------------------------------------------- |
| `typescript-spacetime.md` | TypeScript + SpacetimeDB (React client)       |
| `typescript-postgres.md`  | TypeScript + PostgreSQL (Express + Socket.io) |
| `rust-spacetime.md`       | Rust + SpacetimeDB (CLI client)               |
| `csharp-spacetime.md`     | C# + SpacetimeDB (MAUI client)                |

## Feature Difficulty Comparison

| Feature               | SpacetimeDB | PostgreSQL | Why Different                                                   |
| --------------------- | ----------- | ---------- | --------------------------------------------------------------- |
| Basic chat            | Easy        | Medium     | PG needs WebSocket server + API layer                           |
| Typing indicators     | Trivial     | Hard       | STDB: just a table. PG: WebSocket + Redis pub/sub + cleanup     |
| Read receipts         | Easy        | Hard       | Real-time sync to all clients                                   |
| Unread counts         | Easy        | Hard       | Per-user computed state that syncs                              |
| Scheduled messages    | Trivial     | Hard       | STDB: `scheduleAt()`. PG: external job queue                    |
| Ephemeral messages    | Trivial     | Hard       | STDB: scheduled reducer. PG: job queue + WebSocket              |
| Reactions             | Easy        | Medium     | High-frequency updates to many clients                          |
| Edit history          | Easy        | Medium     | Version syncing                                                 |
| Real-time permissions | Trivial     | Very Hard  | STDB: row delete = instant. PG: session invalidation            |
| Rich presence         | Easy        | Hard       | Heartbeats, cleanup, broadcast                                  |
| Threading             | Easy        | Medium     | Recursive queries + real-time updates                           |
| Private rooms & DMs   | Easy        | Medium     | STDB: row-level security. PG: authorization middleware          |
| Activity indicators   | Trivial     | Hard       | STDB: subscriptions auto-update. PG: polling or complex pub/sub |
| Draft sync            | Trivial     | Hard       | STDB: built-in real-time sync. PG: custom sync infrastructure   |
| Anonymous migration   | Easy        | Hard       | STDB: native identity system. PG: session/token management      |

## Why This Structure?

**Before (redundant):**

```
composed/
├── typescript/spacetime/12_spacetime_anonymous.md  # Same features
├── typescript/postgres/12_postgres_anonymous.md    # Same features
├── rust/spacetime/12_spacetime_anonymous.md        # Same features
├── csharp/spacetime/12_spacetime_anonymous.md      # Same features
```

**After (DRY):**

```
composed/12_full.md           # Features (one source of truth)
language/rust-spacetime.md    # Just setup/architecture (~30 lines)
```

**Benefits:**

- Update features once → applies to all languages
- Language files are tiny - just setup and constraints
- Easy to add new languages (one small file)

## Benchmarking

After generating an app, use the test harness to evaluate it:

```bash
cd ../test-harness
npm install
npx playwright install chromium

# Run benchmark with level matching your prompt
CLIENT_URL=http://localhost:5173 npm run benchmark -- ../staging/typescript/<LLM_MODEL>/spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=12
```

### Prompt Level to `--level` Mapping

| Prompt             | `--level` | Features Scored | Max Score |
| ------------------ | --------- | --------------- | --------- |
| `01_basic`         | 1         | 1-4             | 12        |
| `02_scheduled`     | 2         | 1-5             | 15        |
| `03_realtime`      | 3         | 1-6             | 18        |
| `04_reactions`     | 4         | 1-7             | 21        |
| `05_edit_history`  | 5         | 1-8             | 24        |
| `06_permissions`   | 6         | 1-9             | 27        |
| `07_presence`      | 7         | 1-10            | 30        |
| `08_threading`     | 8         | 1-11            | 33        |
| `09_private_rooms` | 9         | 1-12            | 36        |
| `10_activity`      | 10        | 1-13            | 39        |
| `11_drafts`        | 11        | 1-14            | 42        |
| `12_full`          | 12        | 1-15            | 45        |
