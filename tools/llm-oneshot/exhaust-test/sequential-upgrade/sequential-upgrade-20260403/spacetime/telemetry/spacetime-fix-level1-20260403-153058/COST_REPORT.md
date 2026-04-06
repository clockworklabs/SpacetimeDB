# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T15:30:58-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 27 |
| Total output tokens     | 12,511 |
| Total tokens            | 12,538 |
| Cache read tokens       | 1,643,273 |
| Cache creation tokens   | 43,605 |
| Total cost (USD)        | $0.8442 |
| Total API time          | 226.7s |
| API calls               | 25 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 265 | 20,668 | $0.0544 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 501 | 33,145 | $0.0594 | 10.0s |
| 3 | claude-sonnet-4-6 | 1 | 375 | 54,414 | $0.0238 | 5.9s |
| 4 | claude-sonnet-4-6 | 1 | 516 | 56,474 | $0.0400 | 10.8s |
| 5 | claude-sonnet-4-6 | 1 | 1,253 | 60,564 | $0.0461 | 20.4s |
| 6 | claude-sonnet-4-6 | 1 | 268 | 62,991 | $0.0277 | 9.1s |
| 7 | claude-sonnet-4-6 | 1 | 268 | 64,583 | $0.0260 | 6.6s |
| 8 | claude-sonnet-4-6 | 1 | 509 | 65,278 | $0.0284 | 19.4s |
| 9 | claude-sonnet-4-6 | 1 | 268 | 65,588 | $0.0261 | 6.0s |
| 10 | claude-sonnet-4-6 | 1 | 279 | 66,525 | $0.0269 | 4.8s |
| 11 | claude-sonnet-4-6 | 1 | 275 | 67,591 | $0.0281 | 4.9s |
| 12 | claude-sonnet-4-6 | 1 | 284 | 69,521 | $0.0260 | 6.7s |
| 13 | claude-sonnet-4-6 | 1 | 323 | 69,743 | $0.0270 | 6.3s |
| 14 | claude-sonnet-4-6 | 1 | 217 | 70,069 | $0.0259 | 6.7s |
| 15 | claude-sonnet-4-6 | 1 | 234 | 70,505 | $0.0258 | 4.4s |
| 16 | claude-sonnet-4-6 | 1 | 378 | 70,816 | $0.0281 | 6.4s |
| 17 | claude-sonnet-4-6 | 1 | 464 | 71,144 | $0.0301 | 7.6s |
| 18 | claude-sonnet-4-6 | 1 | 986 | 71,616 | $0.0384 | 10.4s |
| 19 | claude-sonnet-4-6 | 1 | 2,925 | 72,174 | $0.0704 | 32.4s |
| 20 | claude-sonnet-4-6 | 1 | 282 | 73,460 | $0.0376 | 8.6s |
| 21 | claude-sonnet-4-6 | 1 | 155 | 76,479 | $0.0265 | 4.2s |
| 22 | claude-sonnet-4-6 | 1 | 166 | 76,803 | $0.0262 | 3.2s |
| 23 | claude-sonnet-4-6 | 1 | 143 | 77,553 | $0.0259 | 4.2s |
| 24 | claude-sonnet-4-6 | 1 | 911 | 77,692 | $0.0377 | 19.5s |
| 25 | claude-sonnet-4-6 | 1 | 266 | 77,877 | $0.0319 | 4.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
