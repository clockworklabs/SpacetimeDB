# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 4
**Date:** 2026-06-17
**Started:** 2026-06-17T17:19:39-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,428 |
| Total output tokens     | 8,764 |
| Total tokens            | 10,192 |
| Cache read tokens       | 1,301,631 |
| Cache creation tokens   | 53,726 |
| Total cost (USD)        | $0.8456 |
| Total API time          | 141.3s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,405 | 14 | 0 | $0.0015 | 1.7s |
| 2 | claude-sonnet-4-6 | 3 | 366 | 20,621 | $0.0999 | 5.4s |
| 3 | claude-sonnet-4-6 | 1 | 233 | 35,330 | $0.0363 | 4.7s |
| 4 | claude-sonnet-4-6 | 1 | 228 | 39,034 | $0.0433 | 5.6s |
| 5 | claude-sonnet-4-6 | 1 | 1,173 | 43,732 | $0.0987 | 20.1s |
| 6 | claude-sonnet-4-6 | 1 | 823 | 55,058 | $0.0814 | 14.5s |
| 7 | claude-sonnet-4-6 | 1 | 406 | 63,815 | $0.0309 | 6.5s |
| 8 | claude-sonnet-4-6 | 1 | 200 | 64,764 | $0.0256 | 3.7s |
| 9 | claude-sonnet-4-6 | 1 | 207 | 65,296 | $0.0268 | 3.7s |
| 10 | claude-sonnet-4-6 | 1 | 309 | 65,985 | $0.0267 | 4.5s |
| 11 | claude-sonnet-4-6 | 1 | 159 | 66,355 | $0.0275 | 2.6s |
| 12 | claude-sonnet-4-6 | 1 | 431 | 67,220 | $0.0343 | 8.5s |
| 13 | claude-sonnet-4-6 | 1 | 427 | 68,490 | $0.0303 | 5.7s |
| 14 | claude-sonnet-4-6 | 1 | 301 | 69,042 | $0.0284 | 4.3s |
| 15 | claude-sonnet-4-6 | 1 | 621 | 69,571 | $0.0326 | 8.8s |
| 16 | claude-sonnet-4-6 | 1 | 1,211 | 69,974 | $0.0435 | 14.2s |
| 17 | claude-sonnet-4-6 | 1 | 976 | 70,697 | $0.0437 | 12.7s |
| 18 | claude-sonnet-4-6 | 1 | 178 | 72,010 | $0.0313 | 3.4s |
| 19 | claude-sonnet-4-6 | 1 | 171 | 73,187 | $0.0257 | 3.0s |
| 20 | claude-sonnet-4-6 | 1 | 188 | 73,383 | $0.0277 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 122 | 73,855 | $0.0261 | 2.5s |
| 22 | claude-sonnet-4-6 | 1 | 20 | 74,212 | $0.0234 | 2.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
