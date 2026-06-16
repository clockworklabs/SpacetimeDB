# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 12
**Date:** 2026-06-16
**Started:** 2026-06-16T15:11:04-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,765 |
| Total output tokens     | 12,859 |
| Total tokens            | 15,624 |
| Cache read tokens       | 1,901,625 |
| Cache creation tokens   | 58,023 |
| Total cost (USD)        | $0.9838 |
| Total API time          | 204.8s |
| API calls               | 21 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,645 | 14 | 0 | $0.0027 | 1.5s |
| 2 | claude-sonnet-4-6 | 3 | 307 | 20,501 | $0.0599 | 6.8s |
| 3 | claude-sonnet-4-6 | 99 | 143 | 33,594 | $0.0168 | 2.6s |
| 4 | claude-sonnet-4-6 | 1 | 234 | 34,747 | $0.0238 | 4.5s |
| 5 | claude-sonnet-4-6 | 1 | 5,533 | 71,867 | $0.2046 | 90.4s |
| 6 | claude-sonnet-4-6 | 1 | 1,621 | 98,533 | $0.0751 | 21.7s |
| 7 | claude-sonnet-4-6 | 1 | 323 | 104,184 | $0.0426 | 6.9s |
| 8 | claude-sonnet-4-6 | 1 | 694 | 105,923 | $0.0438 | 7.7s |
| 9 | claude-sonnet-4-6 | 1 | 618 | 106,345 | $0.0442 | 7.4s |
| 10 | claude-sonnet-4-6 | 1 | 574 | 107,138 | $0.0438 | 6.6s |
| 11 | claude-sonnet-4-6 | 1 | 674 | 107,954 | $0.0450 | 8.0s |
| 12 | claude-sonnet-4-6 | 1 | 721 | 108,627 | $0.0463 | 9.5s |
| 13 | claude-sonnet-4-6 | 1 | 287 | 109,400 | $0.0402 | 4.0s |
| 14 | claude-sonnet-4-6 | 1 | 153 | 110,220 | $0.0367 | 3.7s |
| 15 | claude-sonnet-4-6 | 1 | 120 | 110,580 | $0.0370 | 3.9s |
| 16 | claude-sonnet-4-6 | 1 | 175 | 111,120 | $0.0365 | 3.0s |
| 17 | claude-sonnet-4-6 | 1 | 183 | 111,272 | $0.0374 | 4.0s |
| 18 | claude-sonnet-4-6 | 1 | 138 | 111,614 | $0.0379 | 3.4s |
| 19 | claude-sonnet-4-6 | 1 | 171 | 112,247 | $0.0368 | 4.3s |
| 20 | claude-sonnet-4-6 | 1 | 137 | 112,400 | $0.0370 | 2.4s |
| 21 | claude-sonnet-4-6 | 1 | 39 | 113,359 | $0.0357 | 2.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
