# AI One-Shot App Generation

This project benchmarks how well Cursor rules enable AI to **one-shot** SpacetimeDB apps — generate and deploy a working app in a single attempt.

**Why isolate from existing apps?** To ensure clean results. If the AI references previous attempts, we can't tell whether success came from the rules or from copying.

---

## The Prompt

```
Read all rules first. Do not reference AI-generated apps in apps/ for guidance.

Execute: @apps/chat-app/prompts/language/<LANGUAGE>.md @apps/chat-app/prompts/composed/<LEVEL>.md
```

**Example (TypeScript, full features):**
```
Read all rules first. Do not reference AI-generated apps in apps/ for guidance.

Execute: @apps/chat-app/prompts/language/typescript-spacetime.md @apps/chat-app/prompts/composed/12_full.md
```

---

## Stacks

| Language File | Stack |
|---------------|-------|
| `typescript-spacetime.md` | TypeScript + SpacetimeDB (React) |
| `typescript-postgres.md` | TypeScript + PostgreSQL (Express) |
| `rust-spacetime.md` | Rust + SpacetimeDB (CLI/egui) |
| `csharp-spacetime.md` | C# + SpacetimeDB (MAUI) |

## Feature Levels

Each level is **cumulative**.

| Level | Features Added |
|-------|----------------|
| 01 | Basic Chat, Typing, Read Receipts, Unread |
| 02 | + Scheduled Messages |
| 03 | + Ephemeral Messages |
| 04 | + Reactions |
| 05 | + Edit History |
| 06 | + Permissions |
| 07 | + Presence |
| 08 | + Threading |
| 09 | + Private Rooms |
| 10 | + Activity Indicators |
| 11 | + Draft Sync |
| 12 | + Anonymous Migration (ALL) |

---

## After Generation

The AI will ask (per `deployment.mdc` rules):
1. **Deploy?** — Local / Cloud / Skip
2. **Benchmark?** — Run E2E tests or AI-grade

For quick AI-based grading (static analysis, no deployment needed):

```
Run AI grading on this app with --level=<N>
```
