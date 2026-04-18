# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-04-07
**Started:** 2026-04-07T09:37:46-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 24 |
| Total output tokens     | 21,133 |
| Total tokens            | 21,157 |
| Cache read tokens       | 1,390,791 |
| Cache creation tokens   | 49,924 |
| Total cost (USD)        | $0.9215 |
| Total API time          | 287.3s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 468 | 20,510 | $0.0581 | 8.1s |
| 2 | claude-sonnet-4-6 | 1 | 148 | 32,483 | $0.0174 | 4.6s |
| 3 | claude-sonnet-4-6 | 1 | 104 | 34,177 | $0.0160 | 2.0s |
| 4 | claude-sonnet-4-6 | 1 | 380 | 34,177 | $0.0290 | 7.3s |
| 5 | claude-sonnet-4-6 | 1 | 1,587 | 44,977 | $0.0741 | 27.3s |
| 6 | claude-sonnet-4-6 | 1 | 230 | 56,396 | $0.0270 | 5.2s |
| 7 | claude-sonnet-4-6 | 1 | 4,754 | 58,159 | $0.0898 | 52.6s |
| 8 | claude-sonnet-4-6 | 1 | 230 | 58,431 | $0.0392 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 194 | 63,282 | $0.0229 | 3.2s |
| 10 | claude-sonnet-4-6 | 1 | 230 | 66,236 | $0.0247 | 5.1s |
| 11 | claude-sonnet-4-6 | 1 | 9,467 | 66,601 | $0.1630 | 109.3s |
| 12 | claude-sonnet-4-6 | 1 | 171 | 66,873 | $0.0585 | 3.5s |
| 13 | claude-sonnet-4-6 | 1 | 160 | 76,931 | $0.0264 | 2.7s |
| 14 | claude-sonnet-4-6 | 1 | 858 | 76,931 | $0.0381 | 11.5s |
| 15 | claude-sonnet-4-6 | 1 | 230 | 77,513 | $0.0303 | 4.5s |
| 16 | claude-sonnet-4-6 | 1 | 172 | 78,482 | $0.0271 | 2.8s |
| 17 | claude-sonnet-4-6 | 1 | 160 | 78,754 | $0.0270 | 2.9s |
| 18 | claude-sonnet-4-6 | 1 | 785 | 78,754 | $0.0378 | 13.6s |
| 19 | claude-sonnet-4-6 | 1 | 173 | 79,395 | $0.0298 | 3.5s |
| 20 | claude-sonnet-4-6 | 1 | 174 | 80,291 | $0.0274 | 4.8s |
| 21 | claude-sonnet-4-6 | 1 | 230 | 80,482 | $0.0294 | 3.8s |
| 22 | claude-sonnet-4-6 | 1 | 228 | 80,956 | $0.0286 | 4.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
