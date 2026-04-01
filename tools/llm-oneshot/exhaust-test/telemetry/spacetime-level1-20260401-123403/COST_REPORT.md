# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-01
**Started:** 2026-04-01T12:34:03-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 21 |
| Total output tokens     | 43,095 |
| Total tokens            | 43,116 |
| Cache read tokens       | 1,292,917 |
| Cache creation tokens   | 45,378 |
| Total cost (USD)        | $1.2045 |
| Total API time          | 616.2s |
| API calls               | 21 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 253 | 35,876 | $0.0215 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 8,000 | 37,724 | $0.1357 | 128.6s |
| 3 | claude-sonnet-4-6 | 1 | 14,273 | 38,902 | $0.2258 | 215.1s |
| 4 | claude-sonnet-4-6 | 1 | 401 | 38,902 | $0.0712 | 5.8s |
| 5 | claude-sonnet-4-6 | 1 | 1,055 | 53,172 | $0.0340 | 13.9s |
| 6 | claude-sonnet-4-6 | 1 | 2,600 | 53,773 | $0.0594 | 26.2s |
| 7 | claude-sonnet-4-6 | 1 | 168 | 54,912 | $0.0291 | 3.8s |
| 8 | claude-sonnet-4-6 | 1 | 224 | 57,596 | $0.0215 | 5.3s |
| 9 | claude-sonnet-4-6 | 1 | 263 | 58,083 | $0.0226 | 4.2s |
| 10 | claude-sonnet-4-6 | 1 | 281 | 58,703 | $0.0288 | 4.6s |
| 11 | claude-sonnet-4-6 | 1 | 546 | 60,560 | $0.0423 | 11.0s |
| 12 | claude-sonnet-4-6 | 1 | 4,547 | 64,817 | $0.0907 | 63.9s |
| 13 | claude-sonnet-4-6 | 1 | 224 | 65,632 | $0.0426 | 4.0s |
| 14 | claude-sonnet-4-6 | 1 | 380 | 70,841 | $0.0280 | 5.4s |
| 15 | claude-sonnet-4-6 | 1 | 4,999 | 71,107 | $0.0980 | 54.9s |
| 16 | claude-sonnet-4-6 | 1 | 3,955 | 71,566 | $0.0998 | 42.0s |
| 17 | claude-sonnet-4-6 | 1 | 224 | 76,644 | $0.0415 | 6.4s |
| 18 | claude-sonnet-4-6 | 1 | 155 | 80,678 | $0.0275 | 3.6s |
| 19 | claude-sonnet-4-6 | 1 | 163 | 80,944 | $0.0275 | 4.4s |
| 20 | claude-sonnet-4-6 | 1 | 191 | 81,152 | $0.0279 | 4.0s |
| 21 | claude-sonnet-4-6 | 1 | 193 | 81,333 | $0.0291 | 4.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
