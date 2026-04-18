# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T15:19:05-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 19 |
| Total output tokens     | 4,651 |
| Total tokens            | 4,670 |
| Cache read tokens       | 586,504 |
| Cache creation tokens   | 26,003 |
| Total cost (USD)        | $0.3433 |
| Total API time          | 74.3s |
| API calls               | 17 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 254 | 20,574 | $0.0339 | 3.3s |
| 2 | claude-sonnet-4-6 | 1 | 145 | 26,954 | $0.0124 | 2.9s |
| 3 | claude-sonnet-4-6 | 1 | 155 | 28,442 | $0.0123 | 2.4s |
| 4 | claude-sonnet-4-6 | 1 | 282 | 28,442 | $0.0175 | 4.0s |
| 5 | claude-sonnet-4-6 | 1 | 155 | 29,709 | $0.0153 | 2.5s |
| 6 | claude-sonnet-4-6 | 1 | 195 | 33,221 | $0.0170 | 3.6s |
| 7 | claude-sonnet-4-6 | 1 | 217 | 34,318 | $0.0178 | 4.0s |
| 8 | claude-sonnet-4-6 | 1 | 414 | 35,445 | $0.0194 | 5.0s |
| 9 | claude-sonnet-4-6 | 1 | 551 | 36,130 | $0.0211 | 8.2s |
| 10 | claude-sonnet-4-6 | 1 | 192 | 36,650 | $0.0167 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 573 | 38,087 | $0.0212 | 6.8s |
| 12 | claude-sonnet-4-6 | 1 | 177 | 38,389 | $0.0167 | 3.0s |
| 13 | claude-sonnet-4-6 | 1 | 344 | 38,389 | $0.0213 | 3.8s |
| 14 | claude-sonnet-4-6 | 1 | 151 | 39,612 | $0.0158 | 2.9s |
| 15 | claude-sonnet-4-6 | 1 | 75 | 40,272 | $0.0138 | 2.2s |
| 16 | claude-sonnet-4-6 | 1 | 72 | 40,765 | $0.0140 | 1.8s |
| 17 | claude-sonnet-4-6 | 1 | 699 | 41,105 | $0.0571 | 14.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
