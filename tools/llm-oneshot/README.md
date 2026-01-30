# AI One-Shot App Generation

This project benchmarks how well Cursor rules enable AI to **one-shot** SpacetimeDB apps — generate and deploy a working app in a single attempt.

## Requirements

- **Cursor IDE** — This benchmark uses Cursor's `@file` reference syntax to compose prompts
- Open this folder (`tools/llm-oneshot`) as your workspace root in Cursor

## Purpose

This benchmark compares AI-generated apps across two platforms:

- **SpacetimeDB** — Real-time database with automatic client sync
- **PostgreSQL** — Traditional database requiring manual WebSocket broadcasting

By generating equivalent apps for both platforms, we can evaluate how well Cursor rules guide the AI to produce working SpacetimeDB applications compared to a familiar baseline (PostgreSQL).

---

## Running a Benchmark

In Cursor, send this prompt to the AI:

```
Read all rules first. Do not reference AI-generated apps in apps/ for guidance.

Execute: @apps/chat-app/prompts/language/<LANGUAGE>.md @apps/chat-app/prompts/composed/<LEVEL>.md
```

**Example (TypeScript + SpacetimeDB, full features):**

```
Read all rules first. Do not reference AI-generated apps in apps/ for guidance.

Execute: @apps/chat-app/prompts/language/typescript-spacetime.md @apps/chat-app/prompts/composed/12_full.md
```

**Example (TypeScript + PostgreSQL, full features):**

```
Read all rules first. Do not reference AI-generated apps in apps/ for guidance.

Execute: @apps/chat-app/prompts/language/typescript-postgres.md @apps/chat-app/prompts/composed/12_full.md
```

**Why isolate from existing apps?** To ensure clean results. If the AI references previous attempts, we can't tell whether success came from the rules or from copying.

---

## Available Stacks

| Language File             | Stack                             |
| ------------------------- | --------------------------------- |
| `typescript-spacetime.md` | TypeScript + SpacetimeDB (React)  |
| `typescript-postgres.md`  | TypeScript + PostgreSQL (Express) |

## Feature Levels

Each level is **cumulative**.

| Level | Features Added                            |
| ----- | ----------------------------------------- |
| 01    | Basic Chat, Typing, Read Receipts, Unread |
| 02    | + Scheduled Messages                      |
| 03    | + Ephemeral Messages                      |
| 04    | + Reactions                               |
| 05    | + Edit History                            |
| 06    | + Permissions                             |
| 07    | + Presence                                |
| 08    | + Threading                               |
| 09    | + Private Rooms                           |
| 10    | + Activity Indicators                     |
| 11    | + Draft Sync                              |
| 12    | + Anonymous Migration (ALL)               |

---

## After Generation

The AI will ask (per `deployment.mdc` rules):

1. **Deploy?** — Local / Cloud / Skip
2. **Grade?** — AI reviews the code and writes a `GRADING_RESULTS.md` file

---

## Grading

Grading is done manually, with AI doing a shallow pass before manual review. The grading rubric is in `apps/{app}/prompts/grading_rubric.md`.

Each graded app gets a `GRADING_RESULTS.md` file in its folder.

### Aggregating Results

To generate summary reports from all graded apps:

```bash
cd tools/llm-oneshot
pnpm install
pnpm run summarize
```

This outputs to `docs/llms/oneshots/`:
- `GRADE_SUMMARY.md` — Executive summary
- `grades.json` — Structured data for websites
- Per-app summaries in `{app}/`

---

## Folder Structure

Generated apps are stored in:

```
apps/{app-name}/{language}/{model}/{platform}/{app-name}-{YYYYMMDD-HHMMSS}/
```

Example:
```
apps/chat-app/typescript/opus-4-5/spacetime/chat-app-20260107-120000/
apps/chat-app/typescript/opus-4-5/postgres/chat-app-20260108-140000/
```

This structure allows comparing results across:
- **Apps** — chat-app, paint-app
- **Models** — opus-4-5, grok-code, gemini-3-pro, gpt-5-2
- **Platforms** — spacetime vs postgres
