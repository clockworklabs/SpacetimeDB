# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T16:09:00-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 19 |
| Total output tokens     | 10,916 |
| Total tokens            | 10,935 |
| Cache read tokens       | 784,759 |
| Cache creation tokens   | 22,686 |
| Total cost (USD)        | $0.4843 |
| Total API time          | 184.7s |
| API calls               | 17 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 266 | 30,161 | $0.0130 | 3.8s |
| 2 | claude-sonnet-4-6 | 1 | 161 | 33,790 | $0.0143 | 3.3s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 33,790 | $0.0192 | 3.4s |
| 4 | claude-sonnet-4-6 | 1 | 241 | 35,573 | $0.0193 | 5.0s |
| 5 | claude-sonnet-4-6 | 1 | 615 | 37,380 | $0.0217 | 10.8s |
| 6 | claude-sonnet-4-6 | 1 | 342 | 37,717 | $0.0262 | 8.7s |
| 7 | claude-sonnet-4-6 | 1 | 490 | 40,324 | $0.0231 | 9.7s |
| 8 | claude-sonnet-4-6 | 1 | 4,241 | 42,957 | $0.0907 | 68.3s |
| 9 | claude-sonnet-4-6 | 1 | 161 | 46,755 | $0.0372 | 3.2s |
| 10 | claude-sonnet-4-6 | 1 | 1,701 | 52,301 | $0.0445 | 28.5s |
| 11 | claude-sonnet-4-6 | 1 | 203 | 53,182 | $0.0262 | 4.0s |
| 12 | claude-sonnet-4-6 | 1 | 528 | 55,095 | $0.0265 | 6.2s |
| 13 | claude-sonnet-4-6 | 1 | 539 | 55,651 | $0.0272 | 5.9s |
| 14 | claude-sonnet-4-6 | 1 | 155 | 56,291 | $0.0216 | 3.0s |
| 15 | claude-sonnet-4-6 | 1 | 173 | 57,366 | $0.0205 | 3.0s |
| 16 | claude-sonnet-4-6 | 1 | 742 | 57,546 | $0.0301 | 13.1s |
| 17 | claude-sonnet-4-6 | 1 | 197 | 58,880 | $0.0228 | 4.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
