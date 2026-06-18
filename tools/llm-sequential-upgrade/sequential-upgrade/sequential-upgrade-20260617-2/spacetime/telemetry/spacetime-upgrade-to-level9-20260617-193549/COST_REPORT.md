# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 9
**Date:** 2026-06-17
**Started:** 2026-06-17T19:35:50-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,438 |
| Total output tokens     | 31,283 |
| Total tokens            | 32,721 |
| Cache read tokens       | 2,011,201 |
| Cache creation tokens   | 63,955 |
| Total cost (USD)        | $1.4577 |
| Total API time          | 387.9s |
| API calls               | 24 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,413 | 17 | 0 | $0.0015 | 1.8s |
| 2 | claude-sonnet-4-6 | 3 | 474 | 20,621 | $0.1026 | 8.7s |
| 3 | claude-sonnet-4-6 | 1 | 218 | 35,504 | $0.0175 | 3.1s |
| 4 | claude-sonnet-4-6 | 1 | 1,823 | 62,805 | $0.0841 | 19.9s |
| 5 | claude-sonnet-4-6 | 1 | 6,644 | 69,118 | $0.1320 | 64.9s |
| 6 | claude-sonnet-4-6 | 1 | 182 | 71,048 | $0.0646 | 3.1s |
| 7 | claude-sonnet-4-6 | 1 | 186 | 77,799 | $0.0274 | 3.1s |
| 8 | claude-sonnet-4-6 | 1 | 658 | 78,016 | $0.0357 | 11.3s |
| 9 | claude-sonnet-4-6 | 1 | 167 | 78,423 | $0.0316 | 2.6s |
| 10 | claude-sonnet-4-6 | 1 | 386 | 79,349 | $0.0312 | 5.9s |
| 11 | claude-sonnet-4-6 | 1 | 195 | 79,612 | $0.0310 | 3.1s |
| 12 | claude-sonnet-4-6 | 1 | 261 | 80,304 | $0.0300 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 290 | 80,644 | $0.0364 | 4.5s |
| 14 | claude-sonnet-4-6 | 1 | 602 | 81,960 | $0.0713 | 11.2s |
| 15 | claude-sonnet-4-6 | 1 | 17,730 | 88,233 | $0.3016 | 208.5s |
| 16 | claude-sonnet-4-6 | 1 | 164 | 89,756 | $0.1364 | 6.0s |
| 17 | claude-sonnet-4-6 | 1 | 137 | 108,217 | $0.0358 | 2.5s |
| 18 | claude-sonnet-4-6 | 1 | 184 | 116,496 | $0.0471 | 3.3s |
| 19 | claude-sonnet-4-6 | 1 | 190 | 118,066 | $0.0403 | 3.0s |
| 20 | claude-sonnet-4-6 | 1 | 172 | 118,403 | $0.0400 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 172 | 118,714 | $0.0393 | 3.9s |
| 22 | claude-sonnet-4-6 | 1 | 283 | 118,905 | $0.0428 | 5.1s |
| 23 | claude-sonnet-4-6 | 1 | 124 | 119,378 | $0.0404 | 2.9s |
| 24 | claude-sonnet-4-6 | 1 | 24 | 119,830 | $0.0371 | 2.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
