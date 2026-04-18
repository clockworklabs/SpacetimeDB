# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-07
**Started:** 2026-04-07T10:56:07-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 20 |
| Total output tokens     | 5,485 |
| Total tokens            | 5,505 |
| Cache read tokens       | 771,537 |
| Cache creation tokens   | 33,617 |
| Total cost (USD)        | $0.4399 |
| Total API time          | 118.7s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 163 | 20,510 | $0.0417 | 4.2s |
| 2 | claude-sonnet-4-6 | 1 | 167 | 29,831 | $0.0535 | 9.7s |
| 3 | claude-sonnet-4-6 | 1 | 159 | 41,042 | $0.0251 | 7.4s |
| 4 | claude-sonnet-4-6 | 1 | 159 | 43,807 | $0.0262 | 5.3s |
| 5 | claude-sonnet-4-6 | 1 | 1,640 | 46,641 | $0.0487 | 27.0s |
| 6 | claude-sonnet-4-6 | 1 | 370 | 51,019 | $0.0228 | 4.5s |
| 7 | claude-sonnet-4-6 | 1 | 518 | 51,545 | $0.0250 | 9.1s |
| 8 | claude-sonnet-4-6 | 1 | 233 | 52,025 | $0.0215 | 5.2s |
| 9 | claude-sonnet-4-6 | 1 | 500 | 52,653 | $0.0243 | 6.2s |
| 10 | claude-sonnet-4-6 | 1 | 515 | 52,928 | $0.0258 | 11.4s |
| 11 | claude-sonnet-4-6 | 1 | 156 | 53,519 | $0.0205 | 3.5s |
| 12 | claude-sonnet-4-6 | 1 | 335 | 54,076 | $0.0219 | 6.8s |
| 13 | claude-sonnet-4-6 | 1 | 122 | 54,250 | $0.0200 | 3.5s |
| 14 | claude-sonnet-4-6 | 1 | 215 | 54,743 | $0.0205 | 4.9s |
| 15 | claude-sonnet-4-6 | 1 | 211 | 55,932 | $0.0231 | 6.1s |
| 16 | claude-sonnet-4-6 | 3 | 22 | 57,016 | $0.0194 | 3.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
