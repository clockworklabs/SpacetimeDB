# LLM One-Shot Benchmark Summary

**Generated:** 2026-01-30
**Total Runs:** 13

## Overall Results by Backend

| Backend | Runs | Avg Score | Best | Worst |
|---------|------|-----------|------|-------|
| SpacetimeDB | 7 | 91.7% | 100.0% | 76.0% |
| PostgreSQL | 6 | 62.7% | 76.4% | 41.7% |

**SpacetimeDB advantage:** +29.0 percentage points

## Results by LLM

| LLM | STDB Runs | STDB Avg | PG Runs | PG Avg | Delta |
|-----|-----------|----------|---------|--------|-------|
| gemini-3-pro | 1 | 85.4% | 1 | 71.9% | +13.5% |
| gpt-5-2 | 1 | 100.0% | 1 | 41.7% | +58.3% |
| grok-code | 1 | 76.0% | 1 | 46.9% | +29.1% |
| opus-4-5 | 4 | 95.1% | 3 | 72.0% | +23.1% |

## Feature Scores (Average)

| Feature | Max | STDB Avg | PG Avg | Winner |
|---------|-----|----------|--------|--------|
| 1. Basic Chat Features | 3 | 2.79 | 1.71 | STDB |
| 2. Typing Indicators | 3 | 2.93 | 2.50 | STDB |
| 3. Read Receipts | 3 | 3.00 | 1.58 | STDB |
| 4. Unread Message Counts | 3 | 2.86 | 1.33 | STDB |
| 5. Scheduled Messages | 3 | 2.36 | 1.75 | STDB |
| 6. Ephemeral/Disappearing Messages | 3 | 2.64 | 2.67 | PG |
| 7. Message Reactions | 3 | 2.89 | 1.79 | STDB |
| 8. Message Editing with History | 3 | 2.93 | 1.83 | STDB |
| 9. Real-Time Permissions | 3 | 2.25 | 1.58 | STDB |
| 10. Rich User Presence | 3 | 2.88 | 2.67 | STDB |
| 11. Message Threading | 3 | 3.00 | 2.00 | STDB |
| 12. Private Rooms & Direct Messages | 3 | 3.00 | 2.17 | STDB |
| 13. Room Activity Indicators | 3 | 0.00 | - | - |
| 14. Draft Sync | 3 | 0.00 | - | - |
| 15. Anonymous to Registered Migration | 3 | 0.00 | - | - |

## All Runs

| LLM | Backend | Date | Score | % | Level |
|-----|---------|------|-------|---|-------|
| gemini-3-pro | PG | 2026-01-08 | 17.25/24 | 71.9% | 5 |
| gemini-3-pro | STDB | 2026-01-07 | 20.5/24 | 85.4% | 5 |
| gpt-5-2 | PG | 2026-01-08 | 10/24 | 41.7% | 5 |
| gpt-5-2 | STDB | 2026-01-07 | 24/24 | 100.0% | 5 |
| grok-code | PG | 2026-01-28 | 11.25/24 | 46.9% | 5 |
| grok-code | STDB | 2026-01-07 | 18.25/24 | 76.0% | 5 |
| opus-4-5 | PG | 2026-01-04 | 27.5/36 | 76.4% | 9 |
| opus-4-5 | PG | 2026-01-04 | 27.25/36 | 75.7% | 9 |
| opus-4-5 | PG | 2026-01-04 | 23/36 | 63.9% | 9 |
| opus-4-5 | STDB | 2026-01-05 | 36/36 | 100.0% | 9 |
| opus-4-5 | STDB | 2026-01-02 | 32.5/36 | 90.3% | 9 |
| opus-4-5 | STDB | 2026-01-02 | 34.5/36 | 95.8% | 9 |
| opus-4-5 | STDB |  | 34/36 | 94.4% | 9 |
