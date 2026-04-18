# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 6
**Date:** 2026-04-07
**Started:** 2026-04-07T10:30:36-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 27 |
| Total output tokens     | 39,714 |
| Total tokens            | 39,741 |
| Cache read tokens       | 2,132,995 |
| Cache creation tokens   | 81,250 |
| Total cost (USD)        | $1.5011 |
| Total API time          | 512.1s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 550 | 20,510 | $0.0596 | 8.9s |
| 2 | claude-haiku-4-5-20251001 | 3 | 558 | 0 | $0.0196 | 2.8s |
| 3 | claude-sonnet-4-6 | 1 | 19,590 | 55,561 | $0.3691 | 241.1s |
| 4 | claude-sonnet-4-6 | 1 | 644 | 71,181 | $0.1045 | 11.9s |
| 5 | claude-sonnet-4-6 | 1 | 244 | 90,771 | $0.0337 | 14.4s |
| 6 | claude-sonnet-4-6 | 1 | 417 | 91,531 | $0.0348 | 7.0s |
| 7 | claude-sonnet-4-6 | 1 | 697 | 91,817 | $0.0400 | 10.9s |
| 8 | claude-sonnet-4-6 | 1 | 1,471 | 92,350 | $0.0528 | 15.1s |
| 9 | claude-sonnet-4-6 | 1 | 244 | 93,144 | $0.0375 | 5.0s |
| 10 | claude-sonnet-4-6 | 1 | 157 | 94,712 | $0.0318 | 3.2s |
| 11 | claude-sonnet-4-6 | 1 | 258 | 98,945 | $0.0344 | 5.0s |
| 12 | claude-sonnet-4-6 | 1 | 11,077 | 99,171 | $0.1970 | 113.7s |
| 13 | claude-sonnet-4-6 | 1 | 1,000 | 99,471 | $0.0867 | 11.7s |
| 14 | claude-sonnet-4-6 | 1 | 244 | 110,640 | $0.0410 | 5.5s |
| 15 | claude-sonnet-4-6 | 1 | 153 | 111,732 | $0.0369 | 5.6s |
| 16 | claude-sonnet-4-6 | 1 | 1,395 | 112,018 | $0.0554 | 19.8s |
| 17 | claude-sonnet-4-6 | 1 | 153 | 112,254 | $0.0415 | 5.9s |
| 18 | claude-sonnet-4-6 | 1 | 159 | 113,740 | $0.0372 | 3.0s |
| 19 | claude-sonnet-4-6 | 1 | 251 | 113,911 | $0.0397 | 6.4s |
| 20 | claude-sonnet-4-6 | 1 | 85 | 114,370 | $0.0367 | 5.6s |
| 21 | claude-sonnet-4-6 | 1 | 101 | 114,902 | $0.0366 | 2.7s |
| 22 | claude-sonnet-4-6 | 1 | 242 | 115,075 | $0.0386 | 3.5s |
| 23 | claude-sonnet-4-6 | 1 | 24 | 115,189 | $0.0360 | 3.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
