# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-06-17
**Started:** 2026-06-17T10:52:40-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,194 |
| Total output tokens     | 26,670 |
| Total tokens            | 28,864 |
| Cache read tokens       | 1,514,021 |
| Cache creation tokens   | 67,430 |
| Total cost (USD)        | $1.2609 |
| Total API time          | 383.4s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,170 | 20 | 0 | $0.0023 | 1.1s |
| 2 | claude-sonnet-4-6 | 3 | 291 | 20,621 | $0.1028 | 6.9s |
| 3 | claude-sonnet-4-6 | 1 | 384 | 35,995 | $0.0329 | 9.5s |
| 4 | claude-sonnet-4-6 | 1 | 262 | 42,129 | $0.0721 | 5.1s |
| 5 | claude-sonnet-4-6 | 1 | 2,219 | 51,380 | $0.0775 | 35.2s |
| 6 | claude-sonnet-4-6 | 1 | 158 | 56,179 | $0.0348 | 4.4s |
| 7 | claude-sonnet-4-6 | 1 | 438 | 58,773 | $0.0439 | 8.1s |
| 8 | claude-sonnet-4-6 | 1 | 413 | 62,054 | $0.0282 | 8.4s |
| 9 | claude-sonnet-4-6 | 1 | 8,233 | 62,616 | $0.1461 | 122.7s |
| 10 | claude-sonnet-4-6 | 1 | 2,294 | 63,252 | $0.1034 | 22.0s |
| 11 | claude-sonnet-4-6 | 1 | 197 | 71,590 | $0.0388 | 3.9s |
| 12 | claude-sonnet-4-6 | 1 | 203 | 73,989 | $0.0300 | 3.2s |
| 13 | claude-sonnet-4-6 | 1 | 294 | 74,774 | $0.0291 | 4.5s |
| 14 | claude-sonnet-4-6 | 1 | 278 | 75,146 | $0.0321 | 3.9s |
| 15 | claude-sonnet-4-6 | 1 | 898 | 76,043 | $0.0428 | 17.1s |
| 16 | claude-sonnet-4-6 | 1 | 157 | 77,133 | $0.0377 | 5.8s |
| 17 | claude-sonnet-4-6 | 1 | 8,602 | 79,165 | $0.1622 | 99.2s |
| 18 | claude-sonnet-4-6 | 1 | 652 | 80,735 | $0.0862 | 8.4s |
| 19 | claude-sonnet-4-6 | 1 | 177 | 89,437 | $0.0341 | 3.4s |
| 20 | claude-sonnet-4-6 | 1 | 170 | 90,208 | $0.0314 | 2.5s |
| 21 | claude-sonnet-4-6 | 1 | 187 | 90,502 | $0.0328 | 3.1s |
| 22 | claude-sonnet-4-6 | 1 | 123 | 90,973 | $0.0313 | 3.2s |
| 23 | claude-sonnet-4-6 | 1 | 20 | 91,327 | $0.0285 | 1.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
