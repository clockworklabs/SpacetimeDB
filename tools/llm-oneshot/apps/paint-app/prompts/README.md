# Paint App Prompt System

Modular prompts for testing SpacetimeDB vs PostgreSQL with a collaborative drawing application.

## Structure

```
prompts/
├── README.md
├── features/               # 16 feature building blocks
│   ├── 01_basic.md
│   ├── 02_cursor_indicators.md
│   ├── ...
│   └── 16_keyboard_shortcuts.md
├── composed/               # Pre-built cumulative prompts (language-agnostic)
│   ├── 01_basic.md
│   ├── 02_shapes.md
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

Example test command:
```bash
# Conceptually: language header + feature prompt
cat language/rust-spacetime.md composed/12_full.md > test_prompt.md
```

Or when using with an LLM:
```
@language/rust-spacetime.md @composed/12_full.md execute
```

## Feature Levels

Each level is **cumulative** — includes all previous features.

| Level | Name | New Features Added |
|-------|------|-------------------|
| 01 | basic | Basic Drawing, Live Cursors |
| 02 | shapes | + Shapes |
| 03 | selection | + Selection & Manipulation |
| 04 | layers | + Layers with Locking |
| 05 | presence | + Presence & Activity Status |
| 06 | comments | + Comments & Feedback |
| 07 | versions | + Version History |
| 08 | permissions | + Permissions |
| 09 | follow | + Follow Mode |
| 10 | activity | + Activity Feed |
| 11 | sharing | + Private Canvases & Sharing |
| 12 | full | + Canvas Chat, Auto-Cleanup, Text & Stickies, Keyboard Shortcuts |

## Language Files

| File | Stack |
|------|-------|
| `typescript-spacetime.md` | TypeScript + SpacetimeDB (React client) |
| `typescript-postgres.md` | TypeScript + PostgreSQL (Express + Socket.io) |
| `rust-spacetime.md` | Rust + SpacetimeDB (axum web GUI) |
| `csharp-spacetime.md` | C# + SpacetimeDB (MAUI client) |

## Features Overview

| # | Feature | Description | STDB Advantage |
|---|---------|-------------|----------------|
| 1 | **Basic Drawing** | Brush, eraser, colors, real-time sync | Foundation |
| 2 | **Live Cursors** | See others' cursors with tool/color preview | High-frequency updates |
| 3 | **Shapes** | Rectangle, ellipse, line, arrow | Standard feature |
| 4 | **Selection** | Select elements, see others' selections | Real-time sync |
| 5 | **Layers + Locking** | Layers with "locked by user" feature | Instant lock/unlock |
| 6 | **Presence** | Active/idle/away status, current tool | Built-in presence |
| 7 | **Comments** | Pin comments, threads, resolve | Standard feature |
| 8 | **Version History** | Auto-save snapshots, restore | Subscription updates |
| 9 | **Permissions** | Viewer/editor roles, instant enforcement | Row-level security |
| 10 | **Follow Mode** | Follow another user's viewport | Viewport state sync |
| 11 | **Activity Feed** | Real-time action log | Subscription-based |
| 12 | **Private Canvases** | Share links, invite users | Access control |
| 13 | **Canvas Chat** | Chat with collaborators, typing indicator | Real-time messaging |
| 14 | **Auto-Cleanup** | Delete inactive canvases after 30 days | Scheduled reducers |
| 15 | **Text & Stickies** | Text labels, sticky notes, inline editing | Real-time sync |
| 16 | **Keyboard Shortcuts** | Tool shortcuts, delete, escape | UX polish |

## Why This Structure?

**Before (redundant):**
```
composed/
├── typescript/spacetime/12_spacetime_full.md  # Same features
├── typescript/postgres/12_postgres_full.md    # Same features  
├── rust/spacetime/12_spacetime_full.md        # Same features
├── csharp/spacetime/12_spacetime_full.md      # Same features
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
