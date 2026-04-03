# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T18:00:45-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 37 |
| Total output tokens     | 16,868 |
| Total tokens            | 16,905 |
| Cache read tokens       | 2,587,398 |
| Cache creation tokens   | 52,884 |
| Total cost (USD)        | $1.2277 |
| Total API time          | 281.9s |
| API calls               | 35 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 288 | 20,668 | $0.0454 | 4.4s |
| 2 | claude-sonnet-4-6 | 1 | 145 | 29,962 | $0.0136 | 4.8s |
| 3 | claude-sonnet-4-6 | 1 | 192 | 34,824 | $0.0561 | 5.2s |
| 4 | claude-sonnet-4-6 | 1 | 161 | 46,230 | $0.0224 | 4.2s |
| 5 | claude-sonnet-4-6 | 1 | 161 | 46,230 | $0.0306 | 5.9s |
| 6 | claude-sonnet-4-6 | 1 | 161 | 50,049 | $0.0280 | 5.4s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 52,878 | $0.0293 | 4.9s |
| 8 | claude-sonnet-4-6 | 1 | 260 | 63,961 | $0.0306 | 5.3s |
| 9 | claude-sonnet-4-6 | 1 | 849 | 66,395 | $0.0345 | 16.5s |
| 10 | claude-sonnet-4-6 | 1 | 296 | 66,894 | $0.0354 | 6.4s |
| 11 | claude-sonnet-4-6 | 1 | 4,262 | 72,530 | $0.0866 | 63.8s |
| 12 | claude-sonnet-4-6 | 1 | 391 | 77,593 | $0.0312 | 6.4s |
| 13 | claude-sonnet-4-6 | 1 | 238 | 78,145 | $0.0286 | 4.2s |
| 14 | claude-sonnet-4-6 | 1 | 238 | 78,850 | $0.0293 | 4.5s |
| 15 | claude-sonnet-4-6 | 1 | 1,485 | 79,413 | $0.0472 | 14.1s |
| 16 | claude-sonnet-4-6 | 1 | 238 | 79,693 | $0.0335 | 3.3s |
| 17 | claude-sonnet-4-6 | 1 | 328 | 79,693 | $0.0359 | 5.0s |
| 18 | claude-sonnet-4-6 | 1 | 408 | 81,570 | $0.0322 | 5.6s |
| 19 | claude-sonnet-4-6 | 1 | 426 | 82,010 | $0.0329 | 6.4s |
| 20 | claude-sonnet-4-6 | 1 | 485 | 82,511 | $0.0340 | 6.2s |
| 21 | claude-sonnet-4-6 | 1 | 773 | 83,030 | $0.0387 | 9.0s |
| 22 | claude-sonnet-4-6 | 1 | 737 | 83,608 | $0.0394 | 8.4s |
| 23 | claude-sonnet-4-6 | 1 | 238 | 84,474 | $0.0327 | 4.1s |
| 24 | claude-sonnet-4-6 | 1 | 170 | 85,495 | $0.0293 | 4.5s |
| 25 | claude-sonnet-4-6 | 1 | 161 | 86,312 | $0.0292 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 1,220 | 86,556 | $0.0455 | 12.8s |
| 27 | claude-sonnet-4-6 | 1 | 238 | 86,894 | $0.0346 | 7.4s |
| 28 | claude-sonnet-4-6 | 1 | 206 | 88,207 | $0.0306 | 3.8s |
| 29 | claude-sonnet-4-6 | 1 | 121 | 88,487 | $0.0298 | 4.0s |
| 30 | claude-sonnet-4-6 | 1 | 97 | 89,005 | $0.0290 | 2.8s |
| 31 | claude-sonnet-4-6 | 1 | 179 | 89,225 | $0.0302 | 5.4s |
| 32 | claude-sonnet-4-6 | 1 | 105 | 89,890 | $0.0298 | 4.8s |
| 33 | claude-sonnet-4-6 | 1 | 268 | 91,592 | $0.0334 | 5.6s |
| 34 | claude-sonnet-4-6 | 1 | 946 | 92,107 | $0.0430 | 19.1s |
| 35 | claude-sonnet-4-6 | 1 | 236 | 92,417 | $0.0352 | 4.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
