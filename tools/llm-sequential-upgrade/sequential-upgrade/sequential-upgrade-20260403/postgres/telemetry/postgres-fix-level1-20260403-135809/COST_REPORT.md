# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T13:58:09-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 25 |
| Total output tokens     | 11,170 |
| Total tokens            | 11,195 |
| Cache read tokens       | 1,223,937 |
| Cache creation tokens   | 23,455 |
| Total cost (USD)        | $0.6228 |
| Total API time          | 175.7s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 266 | 20,668 | $0.0458 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 173 | 44,767 | $0.0217 | 3.4s |
| 3 | claude-sonnet-4-6 | 1 | 200 | 46,507 | $0.0177 | 3.3s |
| 4 | claude-sonnet-4-6 | 1 | 161 | 47,367 | $0.0176 | 2.3s |
| 5 | claude-sonnet-4-6 | 1 | 1,970 | 47,616 | $0.0457 | 35.0s |
| 6 | claude-sonnet-4-6 | 1 | 2,046 | 48,118 | $0.0534 | 31.5s |
| 7 | claude-sonnet-4-6 | 1 | 271 | 52,397 | $0.0223 | 3.6s |
| 8 | claude-sonnet-4-6 | 1 | 235 | 53,066 | $0.0209 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 371 | 53,449 | $0.0235 | 4.0s |
| 10 | claude-sonnet-4-6 | 1 | 206 | 53,956 | $0.0211 | 4.6s |
| 11 | claude-sonnet-4-6 | 1 | 510 | 54,439 | $0.0249 | 7.1s |
| 12 | claude-sonnet-4-6 | 1 | 379 | 54,687 | $0.0244 | 5.4s |
| 13 | claude-sonnet-4-6 | 1 | 206 | 55,290 | $0.0214 | 3.7s |
| 14 | claude-sonnet-4-6 | 1 | 443 | 55,762 | $0.0243 | 6.4s |
| 15 | claude-sonnet-4-6 | 1 | 274 | 56,010 | $0.0229 | 4.2s |
| 16 | claude-sonnet-4-6 | 1 | 499 | 56,913 | $0.0261 | 6.2s |
| 17 | claude-sonnet-4-6 | 1 | 206 | 57,335 | $0.0225 | 5.9s |
| 18 | claude-sonnet-4-6 | 1 | 148 | 59,253 | $0.0218 | 3.0s |
| 19 | claude-sonnet-4-6 | 1 | 718 | 59,740 | $0.0312 | 14.8s |
| 20 | claude-sonnet-4-6 | 1 | 770 | 60,408 | $0.0334 | 7.6s |
| 21 | claude-sonnet-4-6 | 1 | 160 | 61,395 | $0.0240 | 2.8s |
| 22 | claude-sonnet-4-6 | 1 | 754 | 62,237 | $0.0312 | 7.8s |
| 23 | claude-sonnet-4-6 | 1 | 204 | 62,557 | $0.0250 | 4.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
