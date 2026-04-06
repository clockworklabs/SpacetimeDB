# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 4
**Date:** 2026-04-03
**Started:** 2026-04-03T15:02:13-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 32 |
| Total output tokens     | 10,712 |
| Total tokens            | 10,744 |
| Cache read tokens       | 2,218,555 |
| Cache creation tokens   | 32,773 |
| Total cost (USD)        | $0.9492 |
| Total API time          | 175.6s |
| API calls               | 32 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 249 | 39,489 | $0.0195 | 3.8s |
| 2 | claude-sonnet-4-6 | 1 | 249 | 45,936 | $0.0261 | 4.0s |
| 3 | claude-sonnet-4-6 | 1 | 290 | 48,236 | $0.0366 | 6.5s |
| 4 | claude-sonnet-4-6 | 1 | 520 | 52,969 | $0.0613 | 12.6s |
| 5 | claude-sonnet-4-6 | 1 | 461 | 62,991 | $0.0301 | 7.1s |
| 6 | claude-sonnet-4-6 | 1 | 583 | 64,617 | $0.0299 | 10.1s |
| 7 | claude-sonnet-4-6 | 1 | 189 | 65,088 | $0.0250 | 2.6s |
| 8 | claude-sonnet-4-6 | 1 | 345 | 65,789 | $0.0258 | 6.2s |
| 9 | claude-sonnet-4-6 | 1 | 306 | 66,020 | $0.0261 | 3.2s |
| 10 | claude-sonnet-4-6 | 1 | 383 | 66,020 | $0.0288 | 4.1s |
| 11 | claude-sonnet-4-6 | 1 | 316 | 67,376 | $0.0279 | 4.7s |
| 12 | claude-sonnet-4-6 | 1 | 510 | 68,169 | $0.0297 | 7.3s |
| 13 | claude-sonnet-4-6 | 1 | 259 | 68,584 | $0.0274 | 6.1s |
| 14 | claude-sonnet-4-6 | 1 | 189 | 69,377 | $0.0250 | 3.7s |
| 15 | claude-sonnet-4-6 | 1 | 265 | 69,735 | $0.0258 | 6.7s |
| 16 | claude-sonnet-4-6 | 1 | 235 | 69,966 | $0.0259 | 3.7s |
| 17 | claude-sonnet-4-6 | 1 | 322 | 70,325 | $0.0272 | 4.7s |
| 18 | claude-sonnet-4-6 | 1 | 434 | 70,654 | $0.0293 | 5.5s |
| 19 | claude-sonnet-4-6 | 1 | 230 | 71,070 | $0.0268 | 4.3s |
| 20 | claude-sonnet-4-6 | 1 | 400 | 71,598 | $0.0287 | 7.3s |
| 21 | claude-sonnet-4-6 | 1 | 431 | 71,922 | $0.0306 | 5.5s |
| 22 | claude-sonnet-4-6 | 1 | 555 | 72,600 | $0.0321 | 6.2s |
| 23 | claude-sonnet-4-6 | 1 | 1,470 | 73,125 | $0.0464 | 17.6s |
| 24 | claude-sonnet-4-6 | 1 | 174 | 73,774 | $0.0306 | 3.4s |
| 25 | claude-sonnet-4-6 | 1 | 189 | 76,151 | $0.0278 | 4.7s |
| 26 | claude-sonnet-4-6 | 1 | 105 | 76,708 | $0.0255 | 2.8s |
| 27 | claude-sonnet-4-6 | 1 | 182 | 82,079 | $0.0287 | 3.6s |
| 28 | claude-sonnet-4-6 | 1 | 189 | 82,911 | $0.0295 | 4.0s |
| 29 | claude-sonnet-4-6 | 1 | 175 | 83,394 | $0.0284 | 2.9s |
| 30 | claude-sonnet-4-6 | 1 | 109 | 83,601 | $0.0285 | 3.2s |
| 31 | claude-sonnet-4-6 | 1 | 211 | 84,076 | $0.0289 | 4.2s |
| 32 | claude-sonnet-4-6 | 1 | 187 | 84,205 | $0.0297 | 3.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
