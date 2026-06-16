# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 3
**Date:** 2026-06-16
**Started:** 2026-06-16T11:05:21-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,103 |
| Total output tokens     | 12,041 |
| Total tokens            | 14,144 |
| Cache read tokens       | 1,401,940 |
| Cache creation tokens   | 45,072 |
| Total cost (USD)        | $0.7722 |
| Total API time          | 185.1s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,076 | 19 | 0 | $0.0022 | 1.3s |
| 2 | claude-sonnet-4-6 | 3 | 302 | 20,501 | $0.0576 | 5.9s |
| 3 | claude-sonnet-4-6 | 1 | 157 | 33,010 | $0.0331 | 4.6s |
| 4 | claude-sonnet-4-6 | 1 | 755 | 38,574 | $0.0490 | 14.7s |
| 5 | claude-sonnet-4-6 | 1 | 3,098 | 45,546 | $0.0841 | 43.5s |
| 6 | claude-sonnet-4-6 | 1 | 701 | 51,949 | $0.0382 | 7.7s |
| 7 | claude-sonnet-4-6 | 1 | 425 | 55,165 | $0.0264 | 6.6s |
| 8 | claude-sonnet-4-6 | 1 | 288 | 56,083 | $0.0231 | 4.0s |
| 9 | claude-sonnet-4-6 | 1 | 346 | 56,607 | $0.0236 | 4.7s |
| 10 | claude-sonnet-4-6 | 1 | 430 | 56,994 | $0.0252 | 6.5s |
| 11 | claude-sonnet-4-6 | 1 | 346 | 57,439 | $0.0244 | 3.9s |
| 12 | claude-sonnet-4-6 | 1 | 302 | 57,968 | $0.0236 | 4.8s |
| 13 | claude-sonnet-4-6 | 1 | 281 | 58,413 | $0.0236 | 4.5s |
| 14 | claude-sonnet-4-6 | 1 | 387 | 58,913 | $0.0249 | 5.3s |
| 15 | claude-sonnet-4-6 | 1 | 923 | 59,293 | $0.0335 | 9.7s |
| 16 | claude-sonnet-4-6 | 1 | 521 | 59,779 | $0.0296 | 7.3s |
| 17 | claude-sonnet-4-6 | 1 | 467 | 60,801 | $0.0276 | 5.7s |
| 18 | claude-sonnet-4-6 | 1 | 960 | 61,421 | $0.0353 | 10.6s |
| 19 | claude-sonnet-4-6 | 1 | 291 | 62,086 | $0.0270 | 6.4s |
| 20 | claude-sonnet-4-6 | 1 | 152 | 63,145 | $0.0226 | 2.6s |
| 21 | claude-sonnet-4-6 | 1 | 120 | 63,951 | $0.0220 | 2.6s |
| 22 | claude-sonnet-4-6 | 1 | 165 | 64,218 | $0.0227 | 6.6s |
| 23 | claude-sonnet-4-6 | 1 | 92 | 64,469 | $0.0220 | 2.6s |
| 24 | claude-sonnet-4-6 | 1 | 279 | 64,801 | $0.0251 | 5.4s |
| 25 | claude-sonnet-4-6 | 1 | 103 | 65,188 | $0.0227 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 131 | 65,626 | $0.0231 | 4.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
