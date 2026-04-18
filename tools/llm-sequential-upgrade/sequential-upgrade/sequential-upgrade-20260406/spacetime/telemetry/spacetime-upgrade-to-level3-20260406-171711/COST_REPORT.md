# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 3
**Date:** 2026-04-06
**Started:** 2026-04-06T17:17:11-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 133 |
| Total output tokens     | 23,008 |
| Total tokens            | 23,141 |
| Cache read tokens       | 1,504,091 |
| Cache creation tokens   | 51,491 |
| Total cost (USD)        | $0.9898 |
| Total API time          | 334.9s |
| API calls               | 24 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 290 | 20,510 | $0.0551 | 9.0s |
| 2 | claude-sonnet-4-6 | 108 | 231 | 32,400 | $0.0177 | 5.0s |
| 3 | claude-sonnet-4-6 | 1 | 217 | 33,521 | $0.0206 | 7.2s |
| 4 | claude-sonnet-4-6 | 1 | 5,189 | 41,286 | $0.1242 | 74.4s |
| 5 | claude-sonnet-4-6 | 1 | 322 | 50,334 | $0.0399 | 5.7s |
| 6 | claude-sonnet-4-6 | 1 | 1,451 | 55,648 | $0.0398 | 16.6s |
| 7 | claude-sonnet-4-6 | 1 | 262 | 56,004 | $0.0265 | 7.4s |
| 8 | claude-sonnet-4-6 | 1 | 4,067 | 56,004 | $0.0848 | 36.1s |
| 9 | claude-sonnet-4-6 | 1 | 262 | 57,856 | $0.0369 | 6.5s |
| 10 | claude-sonnet-4-6 | 1 | 272 | 63,343 | $0.0246 | 4.8s |
| 11 | claude-sonnet-4-6 | 1 | 201 | 64,056 | $0.0258 | 9.4s |
| 12 | claude-sonnet-4-6 | 1 | 131 | 65,018 | $0.0236 | 4.6s |
| 13 | claude-sonnet-4-6 | 1 | 712 | 65,576 | $0.0348 | 14.6s |
| 14 | claude-sonnet-4-6 | 1 | 7,460 | 66,751 | $0.1347 | 76.4s |
| 15 | claude-sonnet-4-6 | 1 | 173 | 67,503 | $0.0512 | 5.3s |
| 16 | claude-sonnet-4-6 | 1 | 160 | 77,193 | $0.0269 | 3.4s |
| 17 | claude-sonnet-4-6 | 1 | 283 | 77,544 | $0.0290 | 7.7s |
| 18 | claude-sonnet-4-6 | 1 | 262 | 77,938 | $0.0296 | 4.2s |
| 19 | claude-sonnet-4-6 | 1 | 182 | 78,559 | $0.0274 | 4.8s |
| 20 | claude-sonnet-4-6 | 1 | 175 | 78,863 | $0.0270 | 3.7s |
| 21 | claude-sonnet-4-6 | 1 | 107 | 79,063 | $0.0271 | 3.8s |
| 22 | claude-sonnet-4-6 | 1 | 218 | 79,539 | $0.0276 | 4.0s |
| 23 | claude-sonnet-4-6 | 1 | 121 | 79,669 | $0.0266 | 9.2s |
| 24 | claude-sonnet-4-6 | 1 | 260 | 79,913 | $0.0284 | 11.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
