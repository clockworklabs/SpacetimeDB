# Chat App Benchmark Test Harness

Automated testing and benchmarking for LLM-generated chat applications.

## Folder Structure

```
chat-app/
├── staging/                    # Ungraded apps (work in progress)
│   ├── typescript/             # TypeScript SDK tests
│   │   └── <llm-model>/
│   │       └── spacetime|postgres/
│   │           └── chat-app-YYYYMMDD-HHMMSS/
│   ├── rust/                   # Rust SDK tests (future)
│   └── csharp/                 # C# SDK tests (future)
│
├── typescript/                 # Graded results (promoted from staging)
│   └── opus-4-5/
│       └── spacetime/
│           └── chat-app-20260102-162918/
│               └── GRADING_RESULTS.md
├── rust/
├── csharp/
│
├── prompts/                    # Shared benchmark prompts
└── test-harness/              # This folder - testing infrastructure
```

## Quick Start (Full Automation)

```bash
cd apps/chat-app/test-harness
npm install
npx playwright install chromium

# Create a new project in staging:
npm run create -- --lang=typescript --llm=opus-4-5 --backend=spacetime

# Grade it:
npm run grade -- ../staging/typescript/opus-4-5/spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5

# Promote to final location after grading:
npm run promote -- ../staging/typescript/opus-4-5/spacetime/chat-app-YYYYMMDD-HHMMSS/
```

## Workflow

### 1. Create Project (New)

```bash
# Create scaffolded project in staging
npm run create -- --lang=typescript --llm=opus-4-5 --backend=spacetime

# With custom name:
npm run create -- --lang=typescript --llm=gpt-5 --backend=postgres --name=my-test
```

### 2. Implement & Deploy

Have the LLM implement the app in the staging folder, then deploy and test:

```bash
npm run deploy-test -- ../staging/typescript/opus-4-5/spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5
```

### 3. Grade

```bash
# Full grading with AI code review:
npm run grade -- ../staging/typescript/opus-4-5/spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5
```

This will:
1. ✅ Run automated metrics collection
2. ✅ Run AI pattern analysis
3. ✅ Check for E2E test results
4. ✅ Generate summary report
5. ✅ Read the code and provide qualitative review
6. ✅ Identify bugs and edge cases
7. ✅ Recommend final score

### 4. Promote (New)

After grading is complete, promote to the final location:

```bash
npm run promote -- ../staging/typescript/opus-4-5/spacetime/chat-app-YYYYMMDD-HHMMSS/
```

This moves the app from `staging/typescript/...` to `typescript/...` and verifies that `GRADING_RESULTS.md` exists.

## Available Scripts

| Script | Description |
|--------|-------------|
| `npm run create` | Create new project scaffold in staging |
| `npm run deploy-test` | Deploy app and run E2E tests |
| `npm run grade` | Full grading (metrics + AI analysis) |
| `npm run promote` | Move graded app from staging to final |
| `npm run benchmark` | Run E2E tests only (app must be running) |
| `npm run metrics` | Collect code metrics only |
| `npm run ai-grade` | Static code analysis (no deployment) |

## Overview

This test harness evaluates chat app implementations across 14 features:

1. Basic Chat
2. Typing Indicators
3. Read Receipts
4. Unread Message Counts
5. Scheduled Messages
6. Ephemeral/Disappearing Messages
7. Message Reactions
8. Message Editing with History
9. Real-Time Permissions
10. Rich User Presence
11. Message Threading
12. Room Activity Indicators
13. Draft Sync
14. Anonymous to Registered Migration

## Setup

```bash
cd apps/chat-app/test-harness
npm install
npx playwright install chromium
```

## Usage

### Quick Benchmark (Metrics Only)

Collect code metrics without running E2E tests:

```bash
npm run metrics -- ../staging/typescript/opus-4-5/spacetime/chat-app-YYYYMMDD-HHMMSS/
```

This will output:
- Lines of code (backend/frontend)
- File count
- Dependencies
- Compile status

### Full Benchmark (Metrics + E2E Tests)

1. **Start the application** you want to test:
   ```bash
   cd ../spacetime/chat-app-YYYYMMDD-HHMMSS/client
   npm install
   npm run dev
   ```

2. **Run the benchmark** (in a separate terminal):
   ```bash
   # Default: assumes app runs at http://localhost:5173, all 14 features
   npm run benchmark -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/
   
   # Specify prompt level (only evaluate features included in that prompt):
   npm run benchmark -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5
   
   # Custom URL:
   CLIENT_URL=http://localhost:3000 npm run benchmark -- ../path/ --level=8
   ```

### Prompt Levels

Use `--level=N` to match the prompt you used:

| Level | Prompt | Features Evaluated |
|-------|--------|-------------------|
| 1 | `01_*_basic` | 1-4 (Basic, Typing, Receipts, Unread) |
| 2 | `02_*_scheduled` | 1-5 (+ Scheduled) |
| 3 | `03_*_realtime` | 1-6 (+ Ephemeral) |
| 4 | `04_*_reactions` | 1-7 (+ Reactions) |
| 5 | `05_*_edit_history` | 1-8 (+ Edit History) |
| 6 | `06_*_permissions` | 1-9 (+ Permissions) |
| 7 | `07_*_presence` | 1-10 (+ Presence) |
| 8 | `08_*_threading` | 1-11 (+ Threading) |
| 9 | `09_*_activity` | 1-12 (+ Activity Indicators) |
| 10 | `10_*_drafts` | 1-13 (+ Draft Sync) |
| 11 | `11_*_anonymous` | 1-14 (All features) |

**Example:** If you used `05_spacetime_edit_history.md`, run with `--level=5` to only score features 1-8.

### Run Tests Only

If you just want to run the Playwright tests:

```bash
# Run all tests
npm test

# Run with UI (for debugging)
npm run test:ui

# Run specific feature tests
npx playwright test tests/01-basic-chat.spec.ts
```

## Output

### Console Report

```
============================================================
           BENCHMARK RESULTS
============================================================

📁 Project: chat-app-20251229-120000
🏷️  Type: SPACETIME
📅 Date: 12/29/2025, 12:00:00 PM

────────────────────────────────────────────────────────────
                    BUILD STATUS
────────────────────────────────────────────────────────────
Compiles: ✅ PASS
Runs: ✅ PASS

────────────────────────────────────────────────────────────
                    CODE METRICS
────────────────────────────────────────────────────────────
Lines of Code:
  Backend:  312
  Frontend: 535
  Total:    847

────────────────────────────────────────────────────────────
                   FEATURE SCORES
────────────────────────────────────────────────────────────
 1. Basic Chat                 ✅ ███ 3/3 (6/6 tests)
 2. Typing Indicators          ⚠️  ██░ 2/3 (2/3 tests)
 3. Read Receipts              ✅ ███ 3/3 (2/2 tests)
...

────────────────────────────────────────────────────────────
                    TOTAL SCORE: 34/42
────────────────────────────────────────────────────────────
                    GRADE: B (81%)
============================================================
```

### JSON Results

Full results are saved to `results/benchmark-<timestamp>.json`:

```json
{
  "projectPath": "/path/to/project",
  "projectType": "spacetime",
  "metrics": {
    "compiles": true,
    "runs": true,
    "linesOfCode": { "backend": 312, "frontend": 535, "total": 847 },
    "fileCount": { "backend": 8, "frontend": 12, "total": 20 },
    "dependencies": { "backend": ["@spacetimedb/sdk"], "frontend": ["react", "..."] }
  },
  "testResults": [
    { "feature": "Basic Chat", "score": 3, "passed": 6, "total": 6 },
    ...
  ],
  "totalScore": 34,
  "maxScore": 42
}
```

## Comparing SpacetimeDB vs PostgreSQL

Run benchmarks on both implementations and compare:

```bash
# Benchmark SpacetimeDB implementation
npm run benchmark -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/

# Benchmark PostgreSQL implementation  
npm run benchmark -- ../postgres/chat-app-YYYYMMDD-HHMMSS/
```

Results can be compared using the saved JSON files.

## Test Design

Tests are designed to be **implementation-agnostic**:
- Use flexible selectors to find UI elements
- Look for common patterns (buttons, inputs, text)
- Provide soft failures for features that may not exist
- Focus on verifiable behaviors, not specific implementations

## Scoring

Each feature is scored 0-3:

| Score | Meaning |
|-------|---------|
| 0 | Not implemented or completely broken |
| 1 | Partially working (< 40% tests pass) |
| 2 | Mostly working (40-70% tests pass) |
| 3 | Fully working (> 90% tests pass) |

**Maximum score:** 42 (14 features × 3 points)

## Automation Levels

| Command | What it does | When to use |
|---------|--------------|-------------|
| `npm run ai-grade` | Static code analysis (no app running) | Quick prediction in ~5 seconds |
| `npm run benchmark` | E2E tests (app must be running) | Accurate scoring, manual deploy |
| `npm run deploy-test` | Deploy + E2E tests (fully automated) | Complete automation |

### AI-Assisted Grading (Fastest)

Analyzes source code patterns to predict feature completeness without running the app:

```bash
# Quick prediction
npm run ai-grade -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5

# JSON output for scripting
npm run ai-grade -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --json
```

**Pros:** Instant, no deployment needed  
**Cons:** Pattern-based prediction may miss edge cases

### Full Deploy + Test (Most Accurate)

Automatically deploys the app and runs E2E tests:

```bash
# SpacetimeDB app
npm run deploy-test -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5

# PostgreSQL app
npm run deploy-test -- ../postgres/chat-app-YYYYMMDD-HHMMSS/ --level=8

# Keep servers running after tests (for debugging)
npm run deploy-test -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5 --keep
```

**What it does:**
1. Installs dependencies
2. Publishes SpacetimeDB module (or starts Docker for Postgres)
3. Starts client dev server
4. Runs Playwright E2E tests
5. Generates score report
6. Cleans up processes

---

## Customizing Tests

Edit files in `tests/` to:
- Add more specific selectors for your UI
- Adjust timeouts for slower implementations
- Add additional test cases

### Adding New UI Patterns

If AI-generated apps use non-standard UI patterns, add selectors to `tests/helpers.ts`:

```ts
export const SELECTORS = {
  reactionButton: [
    // Add your custom selector here
    'button[data-action="react"]',
    '.my-custom-reaction-btn',
    // ... existing selectors
  ],
};
```

## Troubleshooting

### Tests fail to find elements
- Check that the app is running and accessible
- Verify the CLIENT_URL is correct
- Add more selector patterns to `tests/helpers.ts`
- Use `npm run test:ui` to debug interactively

### Timeouts
- Increase `timeout` in `playwright.config.ts`
- Add longer waits for real-time features

### Compilation fails
- Ensure all dependencies are installed in the target project
- Check for TypeScript errors in the generated code

### Deploy-test fails
- **SpacetimeDB:** Ensure `spacetime start` is running
- **PostgreSQL:** Ensure Docker is running
- Check that ports 5173, 3001 are available
