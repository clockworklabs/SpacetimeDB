# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 5
**Date:** 2026-04-07
**Started:** 2026-04-07T09:37:46-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 34 |
| Total output tokens     | 10,448 |
| Total tokens            | 10,482 |
| Cache read tokens       | 2,106,309 |
| Cache creation tokens   | 28,653 |
| Total cost (USD)        | $0.8962 |
| Total API time          | 195.3s |
| API calls               | 32 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 910 | 43,545 | $0.0785 | 18.7s |
| 2 | claude-sonnet-4-6 | 1 | 439 | 58,305 | $0.0264 | 5.7s |
| 3 | claude-sonnet-4-6 | 1 | 191 | 58,933 | $0.0226 | 5.1s |
| 4 | claude-sonnet-4-6 | 1 | 907 | 59,482 | $0.0323 | 10.1s |
| 5 | claude-sonnet-4-6 | 1 | 410 | 59,715 | $0.0278 | 6.1s |
| 6 | claude-sonnet-4-6 | 1 | 376 | 60,713 | $0.0257 | 8.4s |
| 7 | claude-sonnet-4-6 | 1 | 354 | 61,214 | $0.0254 | 4.5s |
| 8 | claude-sonnet-4-6 | 1 | 191 | 61,681 | $0.0230 | 5.5s |
| 9 | claude-sonnet-4-6 | 1 | 380 | 62,126 | $0.0252 | 5.6s |
| 10 | claude-sonnet-4-6 | 1 | 493 | 62,359 | $0.0279 | 10.0s |
| 11 | claude-sonnet-4-6 | 1 | 696 | 62,830 | $0.0315 | 7.5s |
| 12 | claude-sonnet-4-6 | 1 | 496 | 63,414 | $0.0294 | 6.6s |
| 13 | claude-sonnet-4-6 | 1 | 1,195 | 64,201 | $0.0394 | 12.8s |
| 14 | claude-sonnet-4-6 | 1 | 581 | 64,788 | $0.0330 | 7.5s |
| 15 | claude-sonnet-4-6 | 1 | 175 | 66,074 | $0.0257 | 4.0s |
| 16 | claude-sonnet-4-6 | 1 | 130 | 66,931 | $0.0248 | 3.4s |
| 17 | claude-sonnet-4-6 | 1 | 191 | 68,173 | $0.0243 | 4.4s |
| 18 | claude-sonnet-4-6 | 1 | 152 | 68,173 | $0.0246 | 4.8s |
| 19 | claude-sonnet-4-6 | 1 | 157 | 68,678 | $0.0244 | 3.7s |
| 20 | claude-sonnet-4-6 | 1 | 227 | 69,073 | $0.0256 | 4.9s |
| 21 | claude-sonnet-4-6 | 1 | 246 | 69,455 | $0.0262 | 6.0s |
| 22 | claude-sonnet-4-6 | 1 | 64 | 69,907 | $0.0231 | 2.6s |
| 23 | claude-sonnet-4-6 | 1 | 173 | 70,225 | $0.0243 | 4.4s |
| 24 | claude-sonnet-4-6 | 1 | 168 | 70,398 | $0.0244 | 8.8s |
| 25 | claude-sonnet-4-6 | 1 | 166 | 70,588 | $0.0244 | 4.0s |
| 26 | claude-sonnet-4-6 | 1 | 124 | 70,774 | $0.0248 | 3.1s |
| 27 | claude-sonnet-4-6 | 1 | 178 | 71,231 | $0.0258 | 4.1s |
| 28 | claude-sonnet-4-6 | 1 | 185 | 71,691 | $0.0267 | 5.4s |
| 29 | claude-sonnet-4-6 | 1 | 194 | 72,528 | $0.0251 | 4.8s |
| 30 | claude-sonnet-4-6 | 1 | 189 | 72,635 | $0.0255 | 6.2s |
| 31 | claude-sonnet-4-6 | 1 | 86 | 73,118 | $0.0236 | 2.6s |
| 32 | claude-sonnet-4-6 | 3 | 24 | 73,351 | $0.0247 | 4.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
