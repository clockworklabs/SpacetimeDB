# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 4
**Date:** 2026-04-07
**Started:** 2026-04-07T09:15:40-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 25 |
| Total output tokens     | 7,113 |
| Total tokens            | 7,138 |
| Cache read tokens       | 1,213,782 |
| Cache creation tokens   | 31,653 |
| Total cost (USD)        | $0.5896 |
| Total API time          | 126.0s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 220 | 20,510 | $0.0543 | 6.2s |
| 2 | claude-sonnet-4-6 | 1 | 131 | 32,471 | $0.0155 | 2.4s |
| 3 | claude-sonnet-4-6 | 1 | 424 | 40,099 | $0.0520 | 9.2s |
| 4 | claude-sonnet-4-6 | 1 | 258 | 49,060 | $0.0207 | 4.0s |
| 5 | claude-sonnet-4-6 | 1 | 250 | 49,911 | $0.0212 | 3.0s |
| 6 | claude-sonnet-4-6 | 1 | 855 | 50,570 | $0.0291 | 12.0s |
| 7 | claude-sonnet-4-6 | 1 | 250 | 50,862 | $0.0227 | 4.7s |
| 8 | claude-sonnet-4-6 | 1 | 188 | 52,888 | $0.0223 | 3.1s |
| 9 | claude-sonnet-4-6 | 1 | 250 | 53,848 | $0.0218 | 3.7s |
| 10 | claude-sonnet-4-6 | 1 | 244 | 54,350 | $0.0211 | 5.4s |
| 11 | claude-sonnet-4-6 | 1 | 311 | 54,642 | $0.0223 | 6.9s |
| 12 | claude-sonnet-4-6 | 1 | 215 | 54,978 | $0.0212 | 3.4s |
| 13 | claude-sonnet-4-6 | 1 | 318 | 55,381 | $0.0225 | 7.3s |
| 14 | claude-sonnet-4-6 | 1 | 1,583 | 55,688 | $0.0420 | 20.5s |
| 15 | claude-sonnet-4-6 | 1 | 171 | 56,098 | $0.0257 | 2.5s |
| 16 | claude-sonnet-4-6 | 1 | 250 | 58,865 | $0.0242 | 5.2s |
| 17 | claude-sonnet-4-6 | 1 | 182 | 59,605 | $0.0217 | 3.1s |
| 18 | claude-sonnet-4-6 | 1 | 177 | 59,897 | $0.0214 | 3.0s |
| 19 | claude-sonnet-4-6 | 1 | 102 | 60,097 | $0.0214 | 2.7s |
| 20 | claude-sonnet-4-6 | 1 | 225 | 60,574 | $0.0220 | 4.4s |
| 21 | claude-sonnet-4-6 | 1 | 123 | 60,696 | $0.0210 | 6.0s |
| 22 | claude-sonnet-4-6 | 1 | 138 | 61,096 | $0.0210 | 3.2s |
| 23 | claude-sonnet-4-6 | 1 | 248 | 61,596 | $0.0226 | 4.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
