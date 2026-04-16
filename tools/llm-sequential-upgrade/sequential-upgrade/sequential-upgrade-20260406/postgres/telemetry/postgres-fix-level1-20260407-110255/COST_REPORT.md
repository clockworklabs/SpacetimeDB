# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-07
**Started:** 2026-04-07T11:02:55-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 21 |
| Total output tokens     | 40,453 |
| Total tokens            | 40,474 |
| Cache read tokens       | 1,353,615 |
| Cache creation tokens   | 59,764 |
| Total cost (USD)        | $1.2371 |
| Total API time          | 626.2s |
| API calls               | 19 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 163 | 20,510 | $0.0418 | 3.4s |
| 2 | claude-sonnet-4-6 | 1 | 172 | 29,927 | $0.0549 | 4.9s |
| 3 | claude-sonnet-4-6 | 1 | 159 | 41,487 | $0.0252 | 6.4s |
| 4 | claude-sonnet-4-6 | 1 | 197 | 44,257 | $0.0271 | 5.1s |
| 5 | claude-sonnet-4-6 | 1 | 2,242 | 47,158 | $0.0556 | 31.3s |
| 6 | claude-sonnet-4-6 | 1 | 201 | 49,248 | $0.0369 | 6.1s |
| 7 | claude-sonnet-4-6 | 1 | 159 | 54,332 | $0.0275 | 4.6s |
| 8 | claude-sonnet-4-6 | 1 | 160 | 56,684 | $0.0271 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 13,278 | 58,743 | $0.2223 | 195.3s |
| 10 | claude-sonnet-4-6 | 1 | 20,640 | 60,200 | $0.3866 | 309.8s |
| 11 | claude-sonnet-4-6 | 1 | 676 | 96,564 | $0.0426 | 8.2s |
| 12 | claude-sonnet-4-6 | 1 | 177 | 97,504 | $0.0349 | 3.0s |
| 13 | claude-sonnet-4-6 | 1 | 167 | 98,290 | $0.0328 | 2.9s |
| 14 | claude-sonnet-4-6 | 1 | 155 | 98,509 | $0.0326 | 5.2s |
| 15 | claude-sonnet-4-6 | 1 | 134 | 98,694 | $0.0323 | 4.3s |
| 16 | claude-sonnet-4-6 | 1 | 711 | 99,578 | $0.0411 | 9.7s |
| 17 | claude-sonnet-4-6 | 1 | 202 | 99,732 | $0.0373 | 4.1s |
| 18 | claude-sonnet-4-6 | 1 | 773 | 100,887 | $0.0435 | 15.5s |
| 19 | claude-sonnet-4-6 | 1 | 87 | 101,311 | $0.0350 | 2.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
