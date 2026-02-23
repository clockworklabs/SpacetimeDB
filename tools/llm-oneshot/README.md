# AI One-Shot App Generation

This project benchmarks how well Cursor rules enable AI to **one-shot** SpacetimeDB apps — generate and deploy a working app in a single attempt.

## Purpose

This benchmark compares AI-generated apps across two platforms:

- **SpacetimeDB** — Real-time database with automatic client sync
- **PostgreSQL** — Traditional database requiring manual WebSocket broadcasting

By generating equivalent apps for both platforms, we can evaluate how well Cursor rules guide the AI to produce working SpacetimeDB applications compared to a familiar baseline (PostgreSQL).

---

## How to Run a Benchmark

### Prerequisites

1. Install [Cursor IDE](https://cursor.com/) (free download)
2. Have a Cursor subscription or API credits for the model you want to test
3. For SpacetimeDB tests: install the [SpacetimeDB CLI](https://spacetimedb.com/install)
4. For PostgreSQL tests: have Docker installed (for the database container)

### Step-by-Step Instructions

1. **Open this folder as a workspace in Cursor**
   - File → Open Folder → select `tools/llm-oneshot`
   - This folder must be the workspace root so Cursor loads the `.cursor/rules/` files

2. **Open a new Agent chat**
   - Press `Ctrl+I` (Windows/Linux) or `Cmd+I` (Mac) to open the AI panel
   - Or click the Cursor icon in the sidebar

3. **Select your model**
   - Click the model dropdown at the bottom of the chat panel
   - Choose the model you want to benchmark (e.g., Claude Opus 4.5, GPT-5, Gemini 3 Pro)

4. **Add the prompt files**

   Drag these two files from the file explorer directly into the chat:
   - `apps/chat-app/prompts/language/typescript-spacetime.md` (or your desired stack)
   - `apps/chat-app/prompts/composed/12_full.md` (or your desired feature level)

   Then type this message:

   ```
   Read all rules first. Do not reference AI-generated apps in apps/ for guidance.

   Execute these prompts.
   ```

5. **Let the AI generate the app**
   - Press Enter to send the prompt
   - The AI will read the rules, then generate the backend and client code
   - Do not interrupt — let it complete the full generation

6. **Deploy when prompted**
   - The AI will ask if you want to deploy (Local / Cloud / Skip)
   - Choose "Local" to test the app on your machine

**Why isolate from existing apps?** To ensure clean results. If the AI references previous attempts, we can't tell whether success came from the rules or from copying.

### Example Configurations

**TypeScript + SpacetimeDB (full features):**

- Language: `apps/chat-app/prompts/language/typescript-spacetime.md`
- Level: `apps/chat-app/prompts/composed/12_full.md`

**TypeScript + PostgreSQL (full features):**

- Language: `apps/chat-app/prompts/language/typescript-postgres.md`
- Level: `apps/chat-app/prompts/composed/12_full.md`

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

This outputs to `docs/llms/`:

- `oneshot-summary.md` — Combined summary with feature scores
- `oneshot-grades.json` — Structured data for websites

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
