# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T15:32:46-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 14 |
| Total output tokens     | 3,172 |
| Total tokens            | 3,186 |
| Cache read tokens       | 446,298 |
| Cache creation tokens   | 19,898 |
| Total cost (USD)        | $0.2561 |
| Total API time          | 119.0s |
| API calls               | 12 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 266 | 20,668 | $0.0458 | 8.4s |
| 2 | claude-sonnet-4-6 | 1 | 225 | 31,315 | $0.0245 | 11.7s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 35,437 | $0.0143 | 7.7s |
| 4 | claude-sonnet-4-6 | 1 | 331 | 35,769 | $0.0181 | 12.1s |
| 5 | claude-sonnet-4-6 | 1 | 311 | 36,419 | $0.0223 | 9.5s |
| 6 | claude-sonnet-4-6 | 1 | 417 | 38,201 | $0.0235 | 14.5s |
| 7 | claude-sonnet-4-6 | 1 | 312 | 39,751 | $0.0186 | 11.0s |
| 8 | claude-sonnet-4-6 | 1 | 168 | 40,704 | $0.0166 | 11.2s |
| 9 | claude-sonnet-4-6 | 1 | 182 | 41,664 | $0.0161 | 5.5s |
| 10 | claude-sonnet-4-6 | 1 | 186 | 41,887 | $0.0163 | 7.6s |
| 11 | claude-sonnet-4-6 | 1 | 139 | 42,125 | $0.0156 | 6.2s |
| 12 | claude-sonnet-4-6 | 1 | 474 | 42,358 | $0.0244 | 13.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
