# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 11
**Date:** 2026-04-04
**Started:** 2026-04-04T14:36:04-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 31 |
| Total output tokens     | 9,900 |
| Total tokens            | 9,931 |
| Cache read tokens       | 1,792,631 |
| Cache creation tokens   | 47,193 |
| Total cost (USD)        | $0.8634 |
| Total API time          | 165.9s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 287 | 20,668 | $0.0511 | 5.7s |
| 2 | claude-sonnet-4-6 | 1 | 364 | 33,682 | $0.0337 | 8.5s |
| 3 | claude-sonnet-4-6 | 1 | 376 | 38,507 | $0.0286 | 7.1s |
| 4 | claude-sonnet-4-6 | 1 | 365 | 45,531 | $0.0347 | 6.5s |
| 5 | claude-sonnet-4-6 | 1 | 305 | 49,686 | $0.0325 | 6.9s |
| 6 | claude-sonnet-4-6 | 1 | 305 | 53,169 | $0.0329 | 7.2s |
| 7 | claude-sonnet-4-6 | 1 | 305 | 56,469 | $0.0336 | 5.3s |
| 8 | claude-sonnet-4-6 | 1 | 732 | 59,702 | $0.0375 | 13.8s |
| 9 | claude-sonnet-4-6 | 1 | 192 | 61,988 | $0.0250 | 3.7s |
| 10 | claude-sonnet-4-6 | 1 | 532 | 62,924 | $0.0277 | 5.3s |
| 11 | claude-sonnet-4-6 | 1 | 186 | 63,152 | $0.0242 | 4.6s |
| 12 | claude-sonnet-4-6 | 1 | 777 | 63,796 | $0.0317 | 10.1s |
| 13 | claude-sonnet-4-6 | 1 | 186 | 64,024 | $0.0253 | 2.8s |
| 14 | claude-sonnet-4-6 | 1 | 187 | 64,913 | $0.0231 | 3.1s |
| 15 | claude-sonnet-4-6 | 1 | 581 | 65,141 | $0.0331 | 9.2s |
| 16 | claude-sonnet-4-6 | 1 | 473 | 66,423 | $0.0296 | 6.3s |
| 17 | claude-sonnet-4-6 | 1 | 350 | 67,116 | $0.0275 | 4.6s |
| 18 | claude-sonnet-4-6 | 1 | 317 | 67,682 | $0.0267 | 6.2s |
| 19 | claude-sonnet-4-6 | 1 | 534 | 68,125 | $0.0306 | 7.2s |
| 20 | claude-sonnet-4-6 | 1 | 512 | 68,711 | $0.0306 | 7.2s |
| 21 | claude-sonnet-4-6 | 1 | 316 | 69,943 | $0.0286 | 5.6s |
| 22 | claude-sonnet-4-6 | 1 | 162 | 70,717 | $0.0253 | 3.5s |
| 23 | claude-sonnet-4-6 | 1 | 527 | 71,147 | $0.0318 | 6.4s |
| 24 | claude-sonnet-4-6 | 1 | 186 | 71,830 | $0.0273 | 2.7s |
| 25 | claude-sonnet-4-6 | 1 | 193 | 71,830 | $0.0283 | 3.5s |
| 26 | claude-sonnet-4-6 | 1 | 163 | 72,854 | $0.0257 | 2.9s |
| 27 | claude-sonnet-4-6 | 1 | 90 | 73,564 | $0.0247 | 2.1s |
| 28 | claude-sonnet-4-6 | 1 | 213 | 73,910 | $0.0258 | 5.1s |
| 29 | claude-sonnet-4-6 | 1 | 184 | 75,427 | $0.0259 | 3.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
