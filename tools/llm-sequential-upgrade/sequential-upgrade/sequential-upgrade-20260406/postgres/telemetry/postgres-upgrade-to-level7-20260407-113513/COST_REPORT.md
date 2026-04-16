# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 7
**Date:** 2026-04-07
**Started:** 2026-04-07T11:35:13-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 41 |
| Total output tokens     | 16,789 |
| Total tokens            | 16,830 |
| Cache read tokens       | 2,951,919 |
| Cache creation tokens   | 35,601 |
| Total cost (USD)        | $1.2710 |
| Total API time          | 276.2s |
| API calls               | 41 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 142 | 46,313 | $0.0169 | 3.7s |
| 2 | claude-sonnet-4-6 | 1 | 159 | 46,313 | $0.0250 | 3.9s |
| 3 | claude-sonnet-4-6 | 1 | 159 | 48,630 | $0.0283 | 5.0s |
| 4 | claude-sonnet-4-6 | 1 | 159 | 51,660 | $0.0280 | 4.6s |
| 5 | claude-sonnet-4-6 | 1 | 199 | 54,362 | $0.0254 | 3.3s |
| 6 | claude-sonnet-4-6 | 1 | 159 | 56,002 | $0.0254 | 2.7s |
| 7 | claude-sonnet-4-6 | 1 | 159 | 57,670 | $0.0284 | 5.5s |
| 8 | claude-sonnet-4-6 | 1 | 160 | 59,985 | $0.0314 | 5.8s |
| 9 | claude-sonnet-4-6 | 1 | 3,581 | 62,922 | $0.0760 | 57.2s |
| 10 | claude-sonnet-4-6 | 1 | 196 | 67,447 | $0.0250 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 875 | 67,937 | $0.0344 | 11.5s |
| 12 | claude-sonnet-4-6 | 1 | 671 | 68,175 | $0.0342 | 8.2s |
| 13 | claude-sonnet-4-6 | 1 | 364 | 69,160 | $0.0291 | 4.8s |
| 14 | claude-sonnet-4-6 | 1 | 196 | 69,941 | $0.0256 | 9.2s |
| 15 | claude-sonnet-4-6 | 1 | 355 | 70,396 | $0.0273 | 5.8s |
| 16 | claude-sonnet-4-6 | 1 | 493 | 70,634 | $0.0303 | 6.1s |
| 17 | claude-sonnet-4-6 | 1 | 607 | 71,080 | $0.0326 | 6.6s |
| 18 | claude-sonnet-4-6 | 1 | 370 | 71,664 | $0.0297 | 5.7s |
| 19 | claude-sonnet-4-6 | 1 | 735 | 72,362 | $0.0345 | 9.4s |
| 20 | claude-sonnet-4-6 | 1 | 744 | 72,823 | $0.0368 | 8.1s |
| 21 | claude-sonnet-4-6 | 1 | 1,631 | 73,837 | $0.0498 | 18.0s |
| 22 | claude-sonnet-4-6 | 1 | 210 | 74,672 | $0.0328 | 4.5s |
| 23 | claude-sonnet-4-6 | 1 | 159 | 76,608 | $0.0281 | 3.0s |
| 24 | claude-sonnet-4-6 | 1 | 1,606 | 76,608 | $0.0534 | 16.0s |
| 25 | claude-sonnet-4-6 | 1 | 196 | 78,289 | $0.0328 | 4.6s |
| 26 | claude-sonnet-4-6 | 1 | 185 | 79,986 | $0.0277 | 5.9s |
| 27 | claude-sonnet-4-6 | 1 | 182 | 80,224 | $0.0284 | 3.8s |
| 28 | claude-sonnet-4-6 | 1 | 187 | 80,652 | $0.0286 | 3.6s |
| 29 | claude-sonnet-4-6 | 1 | 195 | 81,077 | $0.0289 | 4.3s |
| 30 | claude-sonnet-4-6 | 1 | 126 | 81,507 | $0.0274 | 3.2s |
| 31 | claude-sonnet-4-6 | 1 | 141 | 81,775 | $0.0276 | 3.5s |
| 32 | claude-sonnet-4-6 | 1 | 103 | 82,019 | $0.0268 | 2.8s |
| 33 | claude-sonnet-4-6 | 1 | 120 | 82,195 | $0.0270 | 2.5s |
| 34 | claude-sonnet-4-6 | 1 | 182 | 82,332 | $0.0286 | 3.9s |
| 35 | claude-sonnet-4-6 | 1 | 172 | 82,653 | $0.0281 | 4.3s |
| 36 | claude-sonnet-4-6 | 1 | 165 | 82,853 | $0.0280 | 3.0s |
| 37 | claude-sonnet-4-6 | 1 | 123 | 83,043 | $0.0285 | 3.8s |
| 38 | claude-sonnet-4-6 | 1 | 204 | 83,498 | $0.0287 | 4.4s |
| 39 | claude-sonnet-4-6 | 1 | 195 | 83,653 | $0.0289 | 3.5s |
| 40 | claude-sonnet-4-6 | 1 | 216 | 84,261 | $0.0302 | 4.3s |
| 41 | claude-sonnet-4-6 | 1 | 8 | 84,701 | $0.0265 | 3.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
