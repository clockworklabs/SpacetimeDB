# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-04-03
**Started:** 2026-04-03T17:47:13-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 28 |
| Total output tokens     | 9,393 |
| Total tokens            | 9,421 |
| Cache read tokens       | 1,950,563 |
| Cache creation tokens   | 27,184 |
| Total cost (USD)        | $0.8281 |
| Total API time          | 195.2s |
| API calls               | 28 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 145 | 47,745 | $0.0173 | 2.6s |
| 2 | claude-sonnet-4-6 | 1 | 162 | 47,964 | $0.0258 | 4.7s |
| 3 | claude-sonnet-4-6 | 1 | 162 | 50,364 | $0.0289 | 5.0s |
| 4 | claude-sonnet-4-6 | 1 | 162 | 53,390 | $0.0292 | 6.1s |
| 5 | claude-sonnet-4-6 | 1 | 162 | 59,711 | $0.0340 | 5.4s |
| 6 | claude-sonnet-4-6 | 1 | 129 | 63,805 | $0.0283 | 3.0s |
| 7 | claude-sonnet-4-6 | 1 | 238 | 66,311 | $0.0245 | 8.5s |
| 8 | claude-sonnet-4-6 | 1 | 243 | 66,868 | $0.0264 | 4.9s |
| 9 | claude-sonnet-4-6 | 1 | 561 | 67,595 | $0.0298 | 9.6s |
| 10 | claude-sonnet-4-6 | 1 | 243 | 67,880 | $0.0266 | 4.0s |
| 11 | claude-sonnet-4-6 | 1 | 243 | 70,928 | $0.0257 | 6.2s |
| 12 | claude-sonnet-4-6 | 1 | 338 | 71,133 | $0.0275 | 7.3s |
| 13 | claude-sonnet-4-6 | 1 | 217 | 71,418 | $0.0263 | 3.9s |
| 14 | claude-sonnet-4-6 | 1 | 234 | 71,850 | $0.0262 | 4.8s |
| 15 | claude-sonnet-4-6 | 1 | 403 | 71,850 | $0.0300 | 6.1s |
| 16 | claude-sonnet-4-6 | 1 | 278 | 72,489 | $0.0278 | 8.8s |
| 17 | claude-sonnet-4-6 | 1 | 661 | 72,986 | $0.0332 | 11.6s |
| 18 | claude-sonnet-4-6 | 1 | 270 | 73,358 | $0.0289 | 6.0s |
| 19 | claude-sonnet-4-6 | 1 | 439 | 74,113 | $0.0309 | 6.3s |
| 20 | claude-sonnet-4-6 | 1 | 496 | 74,677 | $0.0318 | 19.4s |
| 21 | claude-sonnet-4-6 | 1 | 751 | 75,210 | $0.0360 | 10.1s |
| 22 | claude-sonnet-4-6 | 1 | 297 | 75,800 | $0.0304 | 7.4s |
| 23 | claude-sonnet-4-6 | 1 | 545 | 76,645 | $0.0386 | 11.8s |
| 24 | claude-sonnet-4-6 | 1 | 163 | 78,625 | $0.0309 | 4.0s |
| 25 | claude-sonnet-4-6 | 1 | 1,256 | 79,922 | $0.0441 | 15.6s |
| 26 | claude-sonnet-4-6 | 1 | 172 | 82,019 | $0.0295 | 3.8s |
| 27 | claude-sonnet-4-6 | 1 | 159 | 82,628 | $0.0279 | 3.6s |
| 28 | claude-sonnet-4-6 | 1 | 264 | 83,279 | $0.0316 | 4.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
