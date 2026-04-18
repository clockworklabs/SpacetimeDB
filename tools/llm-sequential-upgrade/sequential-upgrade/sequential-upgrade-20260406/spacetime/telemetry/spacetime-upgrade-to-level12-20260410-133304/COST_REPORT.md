# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T13:33:04-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 30 |
| Total output tokens     | 10,208 |
| Total tokens            | 10,238 |
| Cache read tokens       | 1,979,819 |
| Cache creation tokens   | 54,029 |
| Total cost (USD)        | $0.9498 |
| Total API time          | 188.4s |
| API calls               | 28 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 253 | 20,574 | $0.0578 | 6.1s |
| 2 | claude-sonnet-4-6 | 1 | 215 | 33,689 | $0.0238 | 3.5s |
| 3 | claude-sonnet-4-6 | 1 | 796 | 39,311 | $0.0619 | 16.8s |
| 4 | claude-sonnet-4-6 | 1 | 156 | 49,492 | $0.0314 | 7.7s |
| 5 | claude-sonnet-4-6 | 1 | 156 | 53,279 | $0.0285 | 3.5s |
| 6 | claude-sonnet-4-6 | 1 | 705 | 55,982 | $0.0352 | 15.9s |
| 7 | claude-sonnet-4-6 | 1 | 156 | 58,062 | $0.0338 | 3.6s |
| 8 | claude-sonnet-4-6 | 1 | 157 | 64,119 | $0.0335 | 3.7s |
| 9 | claude-sonnet-4-6 | 1 | 1,004 | 70,326 | $0.0496 | 21.5s |
| 10 | claude-sonnet-4-6 | 1 | 229 | 74,944 | $0.0278 | 2.9s |
| 11 | claude-sonnet-4-6 | 1 | 539 | 75,457 | $0.0317 | 8.0s |
| 12 | claude-sonnet-4-6 | 1 | 622 | 75,728 | $0.0345 | 13.8s |
| 13 | claude-sonnet-4-6 | 1 | 229 | 76,379 | $0.0290 | 4.6s |
| 14 | claude-sonnet-4-6 | 1 | 154 | 77,094 | $0.0265 | 2.4s |
| 15 | claude-sonnet-4-6 | 1 | 229 | 78,942 | $0.0299 | 4.7s |
| 16 | claude-sonnet-4-6 | 1 | 331 | 79,672 | $0.0299 | 6.6s |
| 17 | claude-sonnet-4-6 | 1 | 388 | 79,943 | $0.0314 | 5.4s |
| 18 | claude-sonnet-4-6 | 1 | 515 | 80,381 | $0.0336 | 7.1s |
| 19 | claude-sonnet-4-6 | 1 | 529 | 80,857 | $0.0345 | 7.5s |
| 20 | claude-sonnet-4-6 | 1 | 905 | 81,460 | $0.0403 | 10.9s |
| 21 | claude-sonnet-4-6 | 1 | 560 | 82,077 | $0.0375 | 7.1s |
| 22 | claude-sonnet-4-6 | 1 | 378 | 83,272 | $0.0331 | 5.3s |
| 23 | claude-sonnet-4-6 | 1 | 235 | 83,920 | $0.0305 | 4.9s |
| 24 | claude-sonnet-4-6 | 1 | 171 | 84,386 | $0.0289 | 3.0s |
| 25 | claude-sonnet-4-6 | 1 | 163 | 84,663 | $0.0286 | 2.6s |
| 26 | claude-sonnet-4-6 | 1 | 102 | 84,852 | $0.0288 | 2.9s |
| 27 | claude-sonnet-4-6 | 1 | 104 | 85,324 | $0.0283 | 1.9s |
| 28 | claude-sonnet-4-6 | 1 | 227 | 85,634 | $0.0296 | 4.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
