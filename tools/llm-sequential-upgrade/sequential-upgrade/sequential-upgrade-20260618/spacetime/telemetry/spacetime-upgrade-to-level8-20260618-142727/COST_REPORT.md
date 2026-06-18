# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-06-18
**Started:** 2026-06-18T14:27:27-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,395 |
| Total output tokens     | 32,306 |
| Total tokens            | 33,701 |
| Cache read tokens       | 1,370,246 |
| Cache creation tokens   | 52,096 |
| Total cost (USD)        | $1.0923 |
| Total API time          | 443.2s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,380 | 17 | 0 | $0.0015 | 1.1s |
| 2 | claude-sonnet-4-6 | 1 | 612 | 57,117 | $0.0693 | 12.8s |
| 3 | claude-sonnet-4-6 | 1 | 11,264 | 68,567 | $0.1941 | 165.8s |
| 4 | claude-sonnet-4-6 | 1 | 1,604 | 69,788 | $0.1091 | 28.8s |
| 5 | claude-sonnet-4-6 | 1 | 312 | 86,871 | $0.0372 | 4.4s |
| 6 | claude-sonnet-4-6 | 1 | 983 | 88,599 | $0.0430 | 12.3s |
| 7 | claude-sonnet-4-6 | 1 | 223 | 89,035 | $0.0342 | 4.1s |
| 8 | claude-sonnet-4-6 | 1 | 259 | 90,142 | $0.0327 | 3.7s |
| 9 | claude-sonnet-4-6 | 1 | 290 | 90,610 | $0.0343 | 3.9s |
| 10 | claude-sonnet-4-6 | 1 | 14,668 | 92,206 | $0.2494 | 173.5s |
| 11 | claude-sonnet-4-6 | 1 | 1,251 | 92,662 | $0.1019 | 16.2s |
| 12 | claude-sonnet-4-6 | 1 | 174 | 107,430 | $0.0403 | 3.0s |
| 13 | claude-sonnet-4-6 | 1 | 168 | 108,880 | $0.0359 | 2.6s |
| 14 | claude-sonnet-4-6 | 1 | 167 | 109,072 | $0.0370 | 3.4s |
| 15 | claude-sonnet-4-6 | 1 | 164 | 109,540 | $0.0360 | 2.6s |
| 16 | claude-sonnet-4-6 | 1 | 150 | 109,727 | $0.0364 | 5.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
