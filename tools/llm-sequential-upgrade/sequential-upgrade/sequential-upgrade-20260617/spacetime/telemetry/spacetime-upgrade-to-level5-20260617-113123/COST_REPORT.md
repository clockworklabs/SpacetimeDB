# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-06-17
**Started:** 2026-06-17T11:31:24-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,396 |
| Total output tokens     | 16,812 |
| Total tokens            | 19,208 |
| Cache read tokens       | 2,339,798 |
| Cache creation tokens   | 60,690 |
| Total cost (USD)        | $1.3205 |
| Total API time          | 290.2s |
| API calls               | 35 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,360 | 18 | 0 | $0.0024 | 1.8s |
| 2 | claude-sonnet-4-6 | 3 | 321 | 20,621 | $0.1050 | 23.9s |
| 3 | claude-sonnet-4-6 | 1 | 250 | 36,717 | $0.0226 | 6.5s |
| 4 | claude-sonnet-4-6 | 1 | 1,384 | 42,461 | $0.1172 | 26.8s |
| 5 | claude-sonnet-4-6 | 1 | 1,903 | 56,409 | $0.0595 | 29.3s |
| 6 | claude-sonnet-4-6 | 1 | 614 | 58,751 | $0.0443 | 11.8s |
| 7 | claude-sonnet-4-6 | 1 | 330 | 61,666 | $0.0279 | 4.9s |
| 8 | claude-sonnet-4-6 | 1 | 257 | 62,404 | $0.0253 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 349 | 62,858 | $0.0263 | 4.6s |
| 10 | claude-sonnet-4-6 | 1 | 327 | 63,220 | $0.0266 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 326 | 63,674 | $0.0272 | 4.5s |
| 12 | claude-sonnet-4-6 | 1 | 502 | 64,205 | $0.0294 | 7.6s |
| 13 | claude-sonnet-4-6 | 1 | 93 | 64,636 | $0.0244 | 2.6s |
| 14 | claude-sonnet-4-6 | 1 | 193 | 65,379 | $0.0237 | 4.4s |
| 15 | claude-sonnet-4-6 | 1 | 568 | 65,578 | $0.0306 | 12.5s |
| 16 | claude-sonnet-4-6 | 1 | 163 | 65,978 | $0.0266 | 4.0s |
| 17 | claude-sonnet-4-6 | 1 | 376 | 66,711 | $0.0278 | 6.5s |
| 18 | claude-sonnet-4-6 | 1 | 288 | 67,068 | $0.0279 | 4.6s |
| 19 | claude-sonnet-4-6 | 1 | 284 | 67,642 | $0.0272 | 4.2s |
| 20 | claude-sonnet-4-6 | 1 | 297 | 68,078 | $0.0309 | 4.2s |
| 21 | claude-sonnet-4-6 | 1 | 767 | 69,073 | $0.0424 | 15.4s |
| 22 | claude-sonnet-4-6 | 1 | 1,515 | 70,766 | $0.0847 | 23.7s |
| 23 | claude-sonnet-4-6 | 1 | 422 | 77,564 | $0.0424 | 6.5s |
| 24 | claude-sonnet-4-6 | 1 | 216 | 79,697 | $0.0303 | 3.0s |
| 25 | claude-sonnet-4-6 | 1 | 224 | 80,219 | $0.0293 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 418 | 80,535 | $0.0324 | 5.0s |
| 27 | claude-sonnet-4-6 | 1 | 1,937 | 80,859 | $0.0564 | 20.8s |
| 28 | claude-sonnet-4-6 | 1 | 1,046 | 81,377 | $0.0523 | 12.3s |
| 29 | claude-sonnet-4-6 | 1 | 176 | 83,414 | $0.0351 | 3.0s |
| 30 | claude-sonnet-4-6 | 1 | 158 | 84,659 | $0.0289 | 2.4s |
| 31 | claude-sonnet-4-6 | 1 | 167 | 84,854 | $0.0307 | 3.4s |
| 32 | claude-sonnet-4-6 | 1 | 201 | 85,314 | $0.0297 | 3.5s |
| 33 | claude-sonnet-4-6 | 1 | 203 | 85,501 | $0.0308 | 3.2s |
| 34 | claude-sonnet-4-6 | 1 | 249 | 85,847 | $0.0308 | 4.2s |
| 35 | claude-sonnet-4-6 | 1 | 270 | 86,063 | $0.0315 | 8.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
