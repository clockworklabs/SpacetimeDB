# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-06-18
**Started:** 2026-06-18T13:53:49-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,512 |
| Total output tokens     | 37,645 |
| Total tokens            | 39,157 |
| Cache read tokens       | 1,347,807 |
| Cache creation tokens   | 68,598 |
| Total cost (USD)        | $1.2278 |
| Total API time          | 527.6s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,398 | 15 | 0 | $0.0015 | 1.1s |
| 2 | claude-sonnet-4-6 | 100 | 373 | 35,797 | $0.0284 | 5.5s |
| 3 | claude-sonnet-4-6 | 1 | 2,289 | 45,862 | $0.1075 | 43.0s |
| 4 | claude-sonnet-4-6 | 1 | 16,566 | 61,701 | $0.3117 | 242.9s |
| 5 | claude-sonnet-4-6 | 1 | 3,795 | 73,610 | $0.1415 | 40.8s |
| 6 | claude-sonnet-4-6 | 1 | 216 | 90,281 | $0.0450 | 5.0s |
| 7 | claude-sonnet-4-6 | 1 | 199 | 94,181 | $0.0326 | 4.0s |
| 8 | claude-sonnet-4-6 | 1 | 187 | 94,544 | $0.0326 | 4.3s |
| 9 | claude-sonnet-4-6 | 1 | 12,454 | 94,913 | $0.2212 | 149.7s |
| 10 | claude-sonnet-4-6 | 1 | 241 | 96,495 | $0.0796 | 5.0s |
| 11 | claude-sonnet-4-6 | 1 | 512 | 109,049 | $0.0417 | 7.8s |
| 12 | claude-sonnet-4-6 | 1 | 176 | 109,409 | $0.0378 | 4.2s |
| 13 | claude-sonnet-4-6 | 1 | 169 | 110,040 | $0.0363 | 4.2s |
| 14 | claude-sonnet-4-6 | 1 | 167 | 110,234 | $0.0373 | 3.6s |
| 15 | claude-sonnet-4-6 | 1 | 164 | 110,703 | $0.0367 | 3.5s |
| 16 | claude-sonnet-4-6 | 1 | 122 | 110,988 | $0.0364 | 2.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
