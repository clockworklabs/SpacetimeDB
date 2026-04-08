# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-04-03
**Started:** 2026-04-03T16:24:20-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 27 |
| Total output tokens     | 11,017 |
| Total tokens            | 11,044 |
| Cache read tokens       | 1,852,166 |
| Cache creation tokens   | 20,924 |
| Total cost (USD)        | $0.7995 |
| Total API time          | 172.3s |
| API calls               | 27 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 145 | 44,983 | $0.0165 | 2.8s |
| 2 | claude-sonnet-4-6 | 1 | 162 | 44,983 | $0.0260 | 5.1s |
| 3 | claude-sonnet-4-6 | 1 | 2,351 | 55,209 | $0.0663 | 43.1s |
| 4 | claude-sonnet-4-6 | 1 | 226 | 59,066 | $0.0331 | 6.2s |
| 5 | claude-sonnet-4-6 | 1 | 499 | 62,269 | $0.0324 | 9.2s |
| 6 | claude-sonnet-4-6 | 1 | 323 | 63,919 | $0.0260 | 5.2s |
| 7 | claude-sonnet-4-6 | 1 | 247 | 64,453 | $0.0247 | 3.2s |
| 8 | claude-sonnet-4-6 | 1 | 506 | 64,894 | $0.0281 | 6.4s |
| 9 | claude-sonnet-4-6 | 1 | 377 | 65,183 | $0.0276 | 4.0s |
| 10 | claude-sonnet-4-6 | 1 | 431 | 65,807 | $0.0280 | 5.7s |
| 11 | claude-sonnet-4-6 | 1 | 247 | 66,283 | $0.0256 | 3.0s |
| 12 | claude-sonnet-4-6 | 1 | 894 | 69,242 | $0.0352 | 11.0s |
| 13 | claude-sonnet-4-6 | 1 | 945 | 69,505 | $0.0387 | 11.6s |
| 14 | claude-sonnet-4-6 | 1 | 609 | 71,532 | $0.0324 | 9.3s |
| 15 | claude-sonnet-4-6 | 1 | 587 | 72,024 | $0.0331 | 7.7s |
| 16 | claude-sonnet-4-6 | 1 | 398 | 72,727 | $0.0303 | 5.0s |
| 17 | claude-sonnet-4-6 | 1 | 345 | 73,900 | $0.0292 | 4.0s |
| 18 | claude-sonnet-4-6 | 1 | 268 | 75,292 | $0.0274 | 3.8s |
| 19 | claude-sonnet-4-6 | 1 | 155 | 75,499 | $0.0261 | 2.6s |
| 20 | claude-sonnet-4-6 | 1 | 244 | 76,019 | $0.0271 | 3.8s |
| 21 | claude-sonnet-4-6 | 1 | 155 | 76,187 | $0.0265 | 3.4s |
| 22 | claude-sonnet-4-6 | 1 | 160 | 76,525 | $0.0260 | 2.5s |
| 23 | claude-sonnet-4-6 | 1 | 94 | 76,698 | $0.0261 | 2.3s |
| 24 | claude-sonnet-4-6 | 1 | 187 | 77,158 | $0.0264 | 2.9s |
| 25 | claude-sonnet-4-6 | 1 | 198 | 77,274 | $0.0272 | 2.9s |
| 26 | claude-sonnet-4-6 | 1 | 256 | 77,551 | $0.0287 | 4.0s |
| 27 | claude-sonnet-4-6 | 1 | 8 | 77,984 | $0.0246 | 1.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
