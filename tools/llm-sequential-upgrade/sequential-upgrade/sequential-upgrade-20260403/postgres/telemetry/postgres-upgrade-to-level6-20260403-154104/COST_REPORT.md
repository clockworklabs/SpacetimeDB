# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 6
**Date:** 2026-04-03
**Started:** 2026-04-03T15:41:04-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 33 |
| Total output tokens     | 14,416 |
| Total tokens            | 14,449 |
| Cache read tokens       | 1,983,647 |
| Cache creation tokens   | 40,774 |
| Total cost (USD)        | $0.9643 |
| Total API time          | 196.3s |
| API calls               | 31 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 299 | 20,668 | $0.0518 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 1,128 | 42,480 | $0.0739 | 19.2s |
| 3 | claude-sonnet-4-6 | 1 | 598 | 56,028 | $0.0276 | 9.9s |
| 4 | claude-sonnet-4-6 | 1 | 874 | 56,526 | $0.0324 | 8.9s |
| 5 | claude-sonnet-4-6 | 1 | 173 | 57,159 | $0.0234 | 2.7s |
| 6 | claude-sonnet-4-6 | 1 | 714 | 58,145 | $0.0290 | 8.1s |
| 7 | claude-sonnet-4-6 | 1 | 703 | 58,360 | $0.0312 | 6.6s |
| 8 | claude-sonnet-4-6 | 1 | 1,663 | 59,186 | $0.0457 | 15.2s |
| 9 | claude-sonnet-4-6 | 1 | 173 | 59,982 | $0.0272 | 4.0s |
| 10 | claude-sonnet-4-6 | 1 | 250 | 61,738 | $0.0231 | 5.0s |
| 11 | claude-sonnet-4-6 | 1 | 324 | 61,953 | $0.0247 | 4.2s |
| 12 | claude-sonnet-4-6 | 1 | 817 | 62,296 | $0.0325 | 9.6s |
| 13 | claude-sonnet-4-6 | 1 | 464 | 62,713 | $0.0292 | 5.7s |
| 14 | claude-sonnet-4-6 | 1 | 954 | 64,180 | $0.0375 | 9.0s |
| 15 | claude-sonnet-4-6 | 1 | 316 | 65,232 | $0.0289 | 4.9s |
| 16 | claude-sonnet-4-6 | 1 | 393 | 66,456 | $0.0274 | 5.8s |
| 17 | claude-sonnet-4-6 | 1 | 1,038 | 66,865 | $0.0375 | 11.3s |
| 18 | claude-sonnet-4-6 | 1 | 352 | 67,351 | $0.0297 | 5.9s |
| 19 | claude-sonnet-4-6 | 1 | 475 | 68,482 | $0.0293 | 6.6s |
| 20 | claude-sonnet-4-6 | 1 | 173 | 68,927 | $0.0261 | 2.9s |
| 21 | claude-sonnet-4-6 | 1 | 181 | 68,927 | $0.0270 | 4.0s |
| 22 | claude-sonnet-4-6 | 1 | 220 | 69,887 | $0.0256 | 3.7s |
| 23 | claude-sonnet-4-6 | 1 | 112 | 71,647 | $0.0236 | 3.0s |
| 24 | claude-sonnet-4-6 | 1 | 223 | 71,748 | $0.0265 | 3.8s |
| 25 | claude-sonnet-4-6 | 1 | 195 | 72,656 | $0.0252 | 3.6s |
| 26 | claude-sonnet-4-6 | 1 | 168 | 72,793 | $0.0253 | 5.5s |
| 27 | claude-sonnet-4-6 | 1 | 306 | 73,035 | $0.0274 | 4.2s |
| 28 | claude-sonnet-4-6 | 1 | 309 | 73,284 | $0.0303 | 6.1s |
| 29 | claude-sonnet-4-6 | 1 | 498 | 74,260 | $0.0324 | 6.2s |
| 30 | claude-sonnet-4-6 | 1 | 152 | 74,961 | $0.0270 | 2.9s |
| 31 | claude-sonnet-4-6 | 1 | 171 | 75,722 | $0.0260 | 3.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
