# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 3
**Date:** 2026-04-03
**Started:** 2026-04-03T14:28:51-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 28 |
| Total output tokens     | 9,018 |
| Total tokens            | 9,046 |
| Cache read tokens       | 1,441,816 |
| Cache creation tokens   | 23,366 |
| Total cost (USD)        | $0.6555 |
| Total API time          | 150.3s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 156 | 20,668 | $0.0490 | 3.5s |
| 2 | claude-sonnet-4-6 | 1 | 408 | 48,179 | $0.0245 | 6.2s |
| 3 | claude-sonnet-4-6 | 1 | 237 | 49,666 | $0.0204 | 4.8s |
| 4 | claude-sonnet-4-6 | 1 | 237 | 51,460 | $0.0199 | 4.1s |
| 5 | claude-sonnet-4-6 | 1 | 237 | 51,584 | $0.0205 | 4.3s |
| 6 | claude-sonnet-4-6 | 1 | 648 | 51,971 | $0.0266 | 7.7s |
| 7 | claude-sonnet-4-6 | 1 | 876 | 52,320 | $0.0316 | 9.6s |
| 8 | claude-sonnet-4-6 | 1 | 450 | 53,061 | $0.0263 | 7.2s |
| 9 | claude-sonnet-4-6 | 1 | 237 | 54,030 | $0.0218 | 7.6s |
| 10 | claude-sonnet-4-6 | 1 | 292 | 54,573 | $0.0218 | 8.5s |
| 11 | claude-sonnet-4-6 | 1 | 362 | 54,852 | $0.0233 | 5.9s |
| 12 | claude-sonnet-4-6 | 1 | 481 | 55,237 | $0.0255 | 6.6s |
| 13 | claude-sonnet-4-6 | 1 | 623 | 55,692 | $0.0282 | 6.9s |
| 14 | claude-sonnet-4-6 | 1 | 491 | 56,266 | $0.0269 | 5.4s |
| 15 | claude-sonnet-4-6 | 1 | 704 | 56,982 | $0.0298 | 9.9s |
| 16 | claude-sonnet-4-6 | 1 | 173 | 57,566 | $0.0236 | 3.4s |
| 17 | claude-sonnet-4-6 | 1 | 423 | 59,307 | $0.0264 | 6.1s |
| 18 | claude-sonnet-4-6 | 1 | 237 | 59,905 | $0.0235 | 3.6s |
| 19 | claude-sonnet-4-6 | 1 | 136 | 60,421 | $0.0212 | 3.1s |
| 20 | claude-sonnet-4-6 | 1 | 136 | 60,700 | $0.0208 | 2.3s |
| 21 | claude-sonnet-4-6 | 1 | 78 | 61,008 | $0.0202 | 2.7s |
| 22 | claude-sonnet-4-6 | 1 | 317 | 61,347 | $0.0249 | 6.0s |
| 23 | claude-sonnet-4-6 | 1 | 226 | 62,704 | $0.0243 | 5.0s |
| 24 | claude-sonnet-4-6 | 1 | 271 | 63,250 | $0.0254 | 5.7s |
| 25 | claude-sonnet-4-6 | 1 | 235 | 64,454 | $0.0235 | 4.3s |
| 26 | claude-sonnet-4-6 | 1 | 347 | 64,613 | $0.0256 | 10.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
