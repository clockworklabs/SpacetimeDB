# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-04-07
**Started:** 2026-04-07T12:13:30-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 35 |
| Total output tokens     | 12,264 |
| Total tokens            | 12,299 |
| Cache read tokens       | 2,102,627 |
| Cache creation tokens   | 43,033 |
| Total cost (USD)        | $0.9762 |
| Total API time          | 218.6s |
| API calls               | 33 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 321 | 20,510 | $0.0567 | 6.0s |
| 2 | claude-sonnet-4-6 | 1 | 143 | 42,777 | $0.0158 | 2.7s |
| 3 | claude-sonnet-4-6 | 1 | 160 | 42,995 | $0.0266 | 4.8s |
| 4 | claude-sonnet-4-6 | 1 | 160 | 46,011 | $0.0262 | 8.6s |
| 5 | claude-sonnet-4-6 | 1 | 197 | 48,676 | $0.0289 | 5.2s |
| 6 | claude-sonnet-4-6 | 1 | 160 | 51,698 | $0.0294 | 4.6s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 54,756 | $0.0297 | 5.6s |
| 8 | claude-sonnet-4-6 | 1 | 1,901 | 57,654 | $0.0498 | 32.0s |
| 9 | claude-sonnet-4-6 | 1 | 416 | 58,706 | $0.0311 | 8.8s |
| 10 | claude-sonnet-4-6 | 1 | 361 | 60,639 | $0.0256 | 8.0s |
| 11 | claude-sonnet-4-6 | 1 | 287 | 61,171 | $0.0244 | 4.0s |
| 12 | claude-sonnet-4-6 | 1 | 311 | 61,648 | $0.0244 | 4.7s |
| 13 | claude-sonnet-4-6 | 1 | 356 | 61,977 | $0.0255 | 4.4s |
| 14 | claude-sonnet-4-6 | 1 | 287 | 62,857 | $0.0261 | 4.1s |
| 15 | claude-sonnet-4-6 | 1 | 158 | 63,646 | $0.0227 | 3.9s |
| 16 | claude-sonnet-4-6 | 1 | 216 | 65,378 | $0.0266 | 4.5s |
| 17 | claude-sonnet-4-6 | 1 | 287 | 66,380 | $0.0266 | 4.8s |
| 18 | claude-sonnet-4-6 | 1 | 445 | 67,002 | $0.0280 | 7.0s |
| 19 | claude-sonnet-4-6 | 1 | 422 | 67,868 | $0.0288 | 5.7s |
| 20 | claude-sonnet-4-6 | 1 | 1,246 | 68,420 | $0.0411 | 17.7s |
| 21 | claude-sonnet-4-6 | 1 | 430 | 68,934 | $0.0321 | 5.3s |
| 22 | claude-sonnet-4-6 | 1 | 522 | 70,271 | $0.0309 | 8.1s |
| 23 | claude-sonnet-4-6 | 1 | 609 | 70,793 | $0.0335 | 8.4s |
| 24 | claude-sonnet-4-6 | 1 | 1,137 | 71,634 | $0.0412 | 12.4s |
| 25 | claude-sonnet-4-6 | 1 | 171 | 72,335 | $0.0289 | 4.9s |
| 26 | claude-sonnet-4-6 | 1 | 239 | 75,754 | $0.0289 | 5.3s |
| 27 | claude-sonnet-4-6 | 1 | 174 | 76,433 | $0.0265 | 3.1s |
| 28 | claude-sonnet-4-6 | 1 | 298 | 76,690 | $0.0292 | 4.1s |
| 29 | claude-sonnet-4-6 | 1 | 92 | 77,162 | $0.0258 | 2.9s |
| 30 | claude-sonnet-4-6 | 1 | 104 | 77,614 | $0.0255 | 7.7s |
| 31 | claude-sonnet-4-6 | 1 | 105 | 77,798 | $0.0254 | 2.8s |
| 32 | claude-sonnet-4-6 | 1 | 90 | 77,928 | $0.0258 | 2.6s |
| 33 | claude-sonnet-4-6 | 1 | 298 | 78,512 | $0.0285 | 3.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
