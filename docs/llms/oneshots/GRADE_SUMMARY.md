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

## Results by App

| App | STDB Runs | STDB Avg | PG Runs | PG Avg | Delta |
|-----|-----------|----------|---------|--------|-------|
| chat-app | 7 | 91.7% | 6 | 62.7% | +29.0% |

## All Runs

| App | LLM | Backend | Date | Score | % |
|-----|-----|---------|------|-------|---|
| chat-app | gemini-3-pro | PG | 2026-01-08 | 17.25/24 | 71.9% |
| chat-app | gemini-3-pro | STDB | 2026-01-07 | 20.5/24 | 85.4% |
| chat-app | gpt-5-2 | PG | 2026-01-08 | 10/24 | 41.7% |
| chat-app | gpt-5-2 | STDB | 2026-01-07 | 24/24 | 100.0% |
| chat-app | grok-code | PG | 2026-01-28 | 11.25/24 | 46.9% |
| chat-app | grok-code | STDB | 2026-01-07 | 18.25/24 | 76.0% |
| chat-app | opus-4-5 | PG | 2026-01-04 | 27.5/36 | 76.4% |
| chat-app | opus-4-5 | PG | 2026-01-04 | 27.25/36 | 75.7% |
| chat-app | opus-4-5 | PG | 2026-01-04 | 23/36 | 63.9% |
| chat-app | opus-4-5 | STDB | 2026-01-05 | 36/36 | 100.0% |
| chat-app | opus-4-5 | STDB | 2026-01-02 | 32.5/36 | 90.3% |
| chat-app | opus-4-5 | STDB | 2026-01-02 | 34.5/36 | 95.8% |
| chat-app | opus-4-5 | STDB |  | 34/36 | 94.4% |
