# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 9
**Date:** 2026-06-17
**Started:** 2026-06-17T13:32:01-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,645 |
| Total output tokens     | 40,698 |
| Total tokens            | 43,343 |
| Cache read tokens       | 2,415,304 |
| Cache creation tokens   | 74,485 |
| Total cost (USD)        | $1.7845 |
| Total API time          | 540.8s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,620 | 18 | 0 | $0.0027 | 1.4s |
| 2 | claude-sonnet-4-6 | 1 | 206 | 36,652 | $0.0170 | 2.9s |
| 3 | claude-sonnet-4-6 | 1 | 6,138 | 43,529 | $0.2386 | 95.9s |
| 4 | claude-sonnet-4-6 | 1 | 9,127 | 65,780 | $0.2482 | 140.6s |
| 5 | claude-sonnet-4-6 | 1 | 470 | 81,036 | $0.0875 | 7.7s |
| 6 | claude-sonnet-4-6 | 1 | 284 | 90,386 | $0.0349 | 3.8s |
| 7 | claude-sonnet-4-6 | 1 | 707 | 90,980 | $0.0402 | 9.1s |
| 8 | claude-sonnet-4-6 | 1 | 665 | 91,369 | $0.0423 | 6.9s |
| 9 | claude-sonnet-4-6 | 1 | 712 | 92,181 | $0.0430 | 7.1s |
| 10 | claude-sonnet-4-6 | 1 | 1,368 | 92,951 | $0.0533 | 16.1s |
| 11 | claude-sonnet-4-6 | 1 | 201 | 95,340 | $0.0325 | 3.2s |
| 12 | claude-sonnet-4-6 | 1 | 352 | 95,481 | $0.0365 | 7.1s |
| 13 | claude-sonnet-4-6 | 1 | 197 | 95,902 | $0.0349 | 3.4s |
| 14 | claude-sonnet-4-6 | 1 | 269 | 96,426 | $0.0347 | 4.9s |
| 15 | claude-sonnet-4-6 | 1 | 230 | 96,711 | $0.0353 | 4.1s |
| 16 | claude-sonnet-4-6 | 1 | 257 | 97,175 | $0.0353 | 3.5s |
| 17 | claude-sonnet-4-6 | 1 | 140 | 98,858 | $0.0334 | 2.4s |
| 18 | claude-sonnet-4-6 | 1 | 16,790 | 101,869 | $0.2882 | 170.7s |
| 19 | claude-sonnet-4-6 | 1 | 813 | 102,836 | $0.1450 | 10.8s |
| 20 | claude-sonnet-4-6 | 1 | 187 | 119,825 | $0.0443 | 3.8s |
| 21 | claude-sonnet-4-6 | 1 | 417 | 120,757 | $0.0439 | 8.2s |
| 22 | claude-sonnet-4-6 | 1 | 176 | 120,995 | $0.0420 | 3.6s |
| 23 | claude-sonnet-4-6 | 1 | 181 | 121,512 | $0.0403 | 3.6s |
| 24 | claude-sonnet-4-6 | 1 | 294 | 121,707 | $0.0438 | 6.0s |
| 25 | claude-sonnet-4-6 | 1 | 100 | 122,186 | $0.0415 | 2.9s |
| 26 | claude-sonnet-4-6 | 1 | 399 | 122,860 | $0.0452 | 11.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
