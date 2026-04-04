# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-04-04
**Started:** 2026-04-04T12:56:14-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 28 |
| Total output tokens     | 7,470 |
| Total tokens            | 7,498 |
| Cache read tokens       | 1,465,674 |
| Cache creation tokens   | 33,722 |
| Total cost (USD)        | $0.6783 |
| Total API time          | 137.9s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 272 | 20,668 | $0.0507 | 5.5s |
| 2 | claude-sonnet-4-6 | 1 | 144 | 32,359 | $0.0190 | 2.2s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 42,040 | $0.0326 | 5.0s |
| 4 | claude-sonnet-4-6 | 1 | 328 | 46,715 | $0.0279 | 9.1s |
| 5 | claude-sonnet-4-6 | 1 | 161 | 49,107 | $0.0261 | 4.5s |
| 6 | claude-sonnet-4-6 | 1 | 162 | 52,966 | $0.0238 | 3.0s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 55,709 | $0.0210 | 2.5s |
| 8 | claude-sonnet-4-6 | 1 | 604 | 55,709 | $0.0301 | 9.6s |
| 9 | claude-sonnet-4-6 | 1 | 407 | 57,514 | $0.0262 | 4.9s |
| 10 | claude-sonnet-4-6 | 1 | 262 | 58,268 | $0.0234 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 333 | 58,787 | $0.0240 | 4.7s |
| 12 | claude-sonnet-4-6 | 1 | 212 | 59,142 | $0.0225 | 4.2s |
| 13 | claude-sonnet-4-6 | 1 | 244 | 59,568 | $0.0225 | 4.2s |
| 14 | claude-sonnet-4-6 | 1 | 312 | 59,822 | $0.0240 | 4.3s |
| 15 | claude-sonnet-4-6 | 1 | 177 | 60,178 | $0.0222 | 3.9s |
| 16 | claude-sonnet-4-6 | 1 | 494 | 60,583 | $0.0288 | 5.8s |
| 17 | claude-sonnet-4-6 | 1 | 648 | 61,449 | $0.0304 | 8.1s |
| 18 | claude-sonnet-4-6 | 1 | 212 | 62,036 | $0.0246 | 5.3s |
| 19 | claude-sonnet-4-6 | 1 | 161 | 62,777 | $0.0222 | 3.6s |
| 20 | claude-sonnet-4-6 | 1 | 313 | 63,031 | $0.0251 | 4.8s |
| 21 | claude-sonnet-4-6 | 1 | 212 | 63,436 | $0.0237 | 3.4s |
| 22 | claude-sonnet-4-6 | 1 | 152 | 63,842 | $0.0224 | 12.2s |
| 23 | claude-sonnet-4-6 | 1 | 152 | 64,096 | $0.0221 | 2.7s |
| 24 | claude-sonnet-4-6 | 1 | 198 | 64,703 | $0.0231 | 4.4s |
| 25 | claude-sonnet-4-6 | 1 | 980 | 65,328 | $0.0362 | 14.7s |
| 26 | claude-sonnet-4-6 | 1 | 8 | 65,841 | $0.0237 | 1.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
