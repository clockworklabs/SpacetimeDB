# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T14:11:19-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 35 |
| Total output tokens     | 12,404 |
| Total tokens            | 12,439 |
| Cache read tokens       | 1,786,823 |
| Cache creation tokens   | 52,757 |
| Total cost (USD)        | $0.8855 |
| Total API time          | 217.1s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 302 | 20,574 | $0.0508 | 6.4s |
| 2 | claude-haiku-4-5-20251001 | 3 | 469 | 12,857 | $0.0043 | 2.6s |
| 3 | claude-haiku-4-5-20251001 | 3 | 163 | 40,398 | $0.0129 | 2.3s |
| 4 | claude-sonnet-4-6 | 1 | 357 | 31,265 | $0.0212 | 6.1s |
| 5 | claude-sonnet-4-6 | 1 | 156 | 44,055 | $0.0212 | 2.8s |
| 6 | claude-sonnet-4-6 | 1 | 156 | 44,055 | $0.0329 | 4.3s |
| 7 | claude-sonnet-4-6 | 1 | 196 | 48,666 | $0.0305 | 4.8s |
| 8 | claude-sonnet-4-6 | 1 | 156 | 52,131 | $0.0293 | 4.3s |
| 9 | claude-sonnet-4-6 | 1 | 3,576 | 55,152 | $0.0806 | 66.2s |
| 10 | claude-sonnet-4-6 | 1 | 211 | 57,925 | $0.0401 | 3.7s |
| 11 | claude-sonnet-4-6 | 1 | 1,528 | 64,618 | $0.0509 | 30.5s |
| 12 | claude-sonnet-4-6 | 1 | 230 | 66,909 | $0.0294 | 8.0s |
| 13 | claude-sonnet-4-6 | 1 | 716 | 68,471 | $0.0323 | 10.3s |
| 14 | claude-sonnet-4-6 | 1 | 185 | 68,743 | $0.0265 | 3.0s |
| 15 | claude-sonnet-4-6 | 1 | 600 | 69,571 | $0.0307 | 11.4s |
| 16 | claude-sonnet-4-6 | 1 | 175 | 69,798 | $0.0262 | 3.1s |
| 17 | claude-sonnet-4-6 | 1 | 1,269 | 70,505 | $0.0435 | 11.0s |
| 18 | claude-sonnet-4-6 | 1 | 168 | 71,386 | $0.0290 | 2.9s |
| 19 | claude-sonnet-4-6 | 1 | 284 | 72,743 | $0.0274 | 4.9s |
| 20 | claude-sonnet-4-6 | 1 | 185 | 73,090 | $0.0261 | 4.0s |
| 21 | claude-sonnet-4-6 | 1 | 145 | 73,462 | $0.0251 | 3.8s |
| 22 | claude-sonnet-4-6 | 1 | 127 | 73,689 | $0.0249 | 2.8s |
| 23 | claude-sonnet-4-6 | 1 | 127 | 73,689 | $0.0291 | 2.2s |
| 24 | claude-sonnet-4-6 | 1 | 180 | 76,153 | $0.0275 | 2.6s |
| 25 | claude-sonnet-4-6 | 1 | 181 | 76,672 | $0.0265 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 185 | 76,870 | $0.0283 | 2.6s |
| 27 | claude-sonnet-4-6 | 1 | 93 | 77,530 | $0.0255 | 2.5s |
| 28 | claude-sonnet-4-6 | 1 | 101 | 77,757 | $0.0261 | 2.2s |
| 29 | claude-sonnet-4-6 | 1 | 183 | 78,089 | $0.0266 | 2.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
