# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-04-03
**Started:** 2026-04-03T15:18:16-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 24 |
| Total output tokens     | 9,319 |
| Total tokens            | 9,343 |
| Cache read tokens       | 1,272,374 |
| Cache creation tokens   | 31,320 |
| Total cost (USD)        | $0.6390 |
| Total API time          | 138.0s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 290 | 20,668 | $0.0515 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 964 | 43,244 | $0.0659 | 17.8s |
| 3 | claude-sonnet-4-6 | 1 | 440 | 54,510 | $0.0255 | 4.8s |
| 4 | claude-sonnet-4-6 | 1 | 174 | 55,190 | $0.0212 | 3.7s |
| 5 | claude-sonnet-4-6 | 1 | 952 | 55,742 | $0.0318 | 11.3s |
| 6 | claude-sonnet-4-6 | 1 | 399 | 55,958 | $0.0267 | 4.7s |
| 7 | claude-sonnet-4-6 | 1 | 234 | 57,003 | $0.0225 | 5.0s |
| 8 | claude-sonnet-4-6 | 1 | 347 | 57,495 | $0.0235 | 4.3s |
| 9 | claude-sonnet-4-6 | 1 | 356 | 57,771 | $0.0243 | 4.2s |
| 10 | claude-sonnet-4-6 | 1 | 401 | 58,211 | $0.0252 | 4.7s |
| 11 | claude-sonnet-4-6 | 1 | 516 | 58,660 | $0.0272 | 8.3s |
| 12 | claude-sonnet-4-6 | 1 | 1,471 | 59,154 | $0.0421 | 14.7s |
| 13 | claude-sonnet-4-6 | 1 | 536 | 59,763 | $0.0325 | 7.3s |
| 14 | claude-sonnet-4-6 | 1 | 161 | 62,129 | $0.0217 | 4.0s |
| 15 | claude-sonnet-4-6 | 1 | 161 | 63,040 | $0.0222 | 3.3s |
| 16 | claude-sonnet-4-6 | 1 | 924 | 63,040 | $0.0349 | 10.7s |
| 17 | claude-sonnet-4-6 | 1 | 174 | 63,606 | $0.0255 | 4.6s |
| 18 | claude-sonnet-4-6 | 1 | 147 | 64,623 | $0.0224 | 4.5s |
| 19 | claude-sonnet-4-6 | 1 | 154 | 64,839 | $0.0231 | 2.7s |
| 20 | claude-sonnet-4-6 | 1 | 168 | 65,192 | $0.0233 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 154 | 65,506 | $0.0227 | 4.8s |
| 22 | claude-sonnet-4-6 | 1 | 196 | 67,030 | $0.0234 | 5.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
