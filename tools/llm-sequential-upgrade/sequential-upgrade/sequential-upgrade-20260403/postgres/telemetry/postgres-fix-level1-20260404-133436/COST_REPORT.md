# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-04T13:34:36-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 10 |
| Total output tokens     | 3,121 |
| Total tokens            | 3,131 |
| Cache read tokens       | 424,807 |
| Cache creation tokens   | 6,454 |
| Total cost (USD)        | $0.1985 |
| Total API time          | 67.7s |
| API calls               | 10 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 198 | 38,100 | $0.0153 | 9.1s |
| 2 | claude-sonnet-4-6 | 1 | 196 | 38,341 | $0.0154 | 5.5s |
| 3 | claude-sonnet-4-6 | 1 | 345 | 39,561 | $0.0204 | 5.9s |
| 4 | claude-sonnet-4-6 | 1 | 331 | 40,460 | $0.0250 | 5.1s |
| 5 | claude-sonnet-4-6 | 1 | 218 | 42,568 | $0.0191 | 4.0s |
| 6 | claude-sonnet-4-6 | 1 | 507 | 44,075 | $0.0222 | 6.2s |
| 7 | claude-sonnet-4-6 | 1 | 282 | 44,447 | $0.0199 | 4.8s |
| 8 | claude-sonnet-4-6 | 1 | 160 | 45,066 | $0.0174 | 5.7s |
| 9 | claude-sonnet-4-6 | 1 | 176 | 45,897 | $0.0179 | 5.1s |
| 10 | claude-sonnet-4-6 | 1 | 708 | 46,292 | $0.0259 | 16.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
