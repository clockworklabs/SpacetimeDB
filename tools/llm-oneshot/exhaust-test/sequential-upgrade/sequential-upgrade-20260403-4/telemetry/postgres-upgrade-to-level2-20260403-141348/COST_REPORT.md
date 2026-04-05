# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-04-03
**Started:** 2026-04-03T14:13:49-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 19 |
| Total output tokens     | 8,795 |
| Total tokens            | 8,814 |
| Cache read tokens       | 988,389 |
| Cache creation tokens   | 18,749 |
| Total cost (USD)        | $0.4988 |
| Total API time          | 196.2s |
| API calls               | 19 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 761 | 38,149 | $0.0488 | 15.1s |
| 2 | claude-sonnet-4-6 | 1 | 743 | 45,054 | $0.0298 | 13.9s |
| 3 | claude-sonnet-4-6 | 1 | 226 | 46,432 | $0.0209 | 5.2s |
| 4 | claude-sonnet-4-6 | 1 | 1,336 | 47,391 | $0.0355 | 15.2s |
| 5 | claude-sonnet-4-6 | 1 | 294 | 47,729 | $0.0242 | 7.5s |
| 6 | claude-sonnet-4-6 | 1 | 417 | 49,564 | $0.0227 | 19.9s |
| 7 | claude-sonnet-4-6 | 1 | 305 | 49,989 | $0.0215 | 7.1s |
| 8 | claude-sonnet-4-6 | 1 | 772 | 50,499 | $0.0282 | 11.1s |
| 9 | claude-sonnet-4-6 | 1 | 1,007 | 50,897 | $0.0340 | 13.1s |
| 10 | claude-sonnet-4-6 | 1 | 170 | 51,866 | $0.0222 | 5.8s |
| 11 | claude-sonnet-4-6 | 1 | 161 | 53,708 | $0.0194 | 5.8s |
| 12 | claude-sonnet-4-6 | 1 | 1,291 | 53,927 | $0.0371 | 18.5s |
| 13 | claude-sonnet-4-6 | 1 | 179 | 54,334 | $0.0242 | 7.4s |
| 14 | claude-sonnet-4-6 | 1 | 167 | 55,718 | $0.0205 | 4.0s |
| 15 | claude-sonnet-4-6 | 1 | 171 | 56,346 | $0.0201 | 7.3s |
| 16 | claude-sonnet-4-6 | 1 | 231 | 57,249 | $0.0237 | 17.6s |
| 17 | claude-sonnet-4-6 | 1 | 67 | 59,454 | $0.0202 | 6.7s |
| 18 | claude-sonnet-4-6 | 1 | 198 | 59,813 | $0.0213 | 5.8s |
| 19 | claude-sonnet-4-6 | 1 | 299 | 60,270 | $0.0245 | 9.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
