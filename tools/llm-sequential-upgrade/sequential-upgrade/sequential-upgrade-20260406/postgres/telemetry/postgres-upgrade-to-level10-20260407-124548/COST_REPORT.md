# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 10
**Date:** 2026-04-07
**Started:** 2026-04-07T12:45:49-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 24 |
| Total output tokens     | 7,330 |
| Total tokens            | 7,354 |
| Cache read tokens       | 1,162,404 |
| Cache creation tokens   | 31,237 |
| Total cost (USD)        | $0.5759 |
| Total API time          | 129.7s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 717 | 20,510 | $0.0558 | 12.0s |
| 2 | claude-sonnet-4-6 | 1 | 278 | 30,867 | $0.0256 | 5.1s |
| 3 | claude-sonnet-4-6 | 1 | 267 | 36,262 | $0.0164 | 4.9s |
| 4 | claude-sonnet-4-6 | 1 | 407 | 36,671 | $0.0273 | 9.3s |
| 5 | claude-sonnet-4-6 | 1 | 443 | 44,451 | $0.0231 | 8.9s |
| 6 | claude-sonnet-4-6 | 1 | 290 | 45,288 | $0.0265 | 4.0s |
| 7 | claude-sonnet-4-6 | 1 | 220 | 49,448 | $0.0308 | 5.6s |
| 8 | claude-sonnet-4-6 | 1 | 159 | 52,836 | $0.0245 | 6.1s |
| 9 | claude-sonnet-4-6 | 1 | 390 | 56,425 | $0.0257 | 5.9s |
| 10 | claude-sonnet-4-6 | 1 | 658 | 57,208 | $0.0293 | 11.6s |
| 11 | claude-sonnet-4-6 | 1 | 329 | 57,812 | $0.0251 | 4.5s |
| 12 | claude-sonnet-4-6 | 1 | 426 | 58,561 | $0.0255 | 5.2s |
| 13 | claude-sonnet-4-6 | 1 | 431 | 58,981 | $0.0261 | 5.7s |
| 14 | claude-sonnet-4-6 | 1 | 788 | 59,498 | $0.0316 | 8.5s |
| 15 | claude-sonnet-4-6 | 1 | 159 | 61,003 | $0.0215 | 3.2s |
| 16 | claude-sonnet-4-6 | 1 | 341 | 61,212 | $0.0250 | 4.2s |
| 17 | claude-sonnet-4-6 | 1 | 304 | 61,605 | $0.0247 | 3.7s |
| 18 | claude-sonnet-4-6 | 1 | 158 | 62,037 | $0.0224 | 3.8s |
| 19 | claude-sonnet-4-6 | 1 | 119 | 62,414 | $0.0222 | 3.4s |
| 20 | claude-sonnet-4-6 | 1 | 217 | 62,862 | $0.0231 | 5.5s |
| 21 | claude-sonnet-4-6 | 1 | 195 | 63,116 | $0.0227 | 3.6s |
| 22 | claude-sonnet-4-6 | 1 | 34 | 63,337 | $0.0211 | 4.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
