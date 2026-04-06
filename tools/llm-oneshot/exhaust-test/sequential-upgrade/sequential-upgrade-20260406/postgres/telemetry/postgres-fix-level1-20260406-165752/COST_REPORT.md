# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-06
**Started:** 2026-04-06T16:57:52-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 17 |
| Total output tokens     | 7,044 |
| Total tokens            | 7,061 |
| Cache read tokens       | 764,341 |
| Cache creation tokens   | 21,782 |
| Total cost (USD)        | $0.4167 |
| Total API time          | 123.2s |
| API calls               | 15 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 167 | 20,510 | $0.0427 | 2.6s |
| 2 | claude-sonnet-4-6 | 1 | 233 | 29,588 | $0.0149 | 4.6s |
| 3 | claude-sonnet-4-6 | 1 | 217 | 48,596 | $0.0206 | 3.9s |
| 4 | claude-sonnet-4-6 | 1 | 468 | 49,945 | $0.0249 | 7.9s |
| 5 | claude-sonnet-4-6 | 1 | 575 | 50,704 | $0.0274 | 9.2s |
| 6 | claude-sonnet-4-6 | 1 | 260 | 51,646 | $0.0222 | 3.9s |
| 7 | claude-sonnet-4-6 | 1 | 943 | 52,399 | $0.0313 | 15.0s |
| 8 | claude-sonnet-4-6 | 1 | 619 | 52,775 | $0.0287 | 9.5s |
| 9 | claude-sonnet-4-6 | 1 | 700 | 53,739 | $0.0292 | 12.9s |
| 10 | claude-sonnet-4-6 | 1 | 924 | 54,413 | $0.0340 | 15.5s |
| 11 | claude-sonnet-4-6 | 1 | 335 | 56,446 | $0.0256 | 10.0s |
| 12 | claude-sonnet-4-6 | 1 | 284 | 58,157 | $0.0304 | 4.7s |
| 13 | claude-sonnet-4-6 | 1 | 124 | 61,040 | $0.0220 | 2.7s |
| 14 | claude-sonnet-4-6 | 1 | 571 | 61,530 | $0.0320 | 11.4s |
| 15 | claude-sonnet-4-6 | 1 | 624 | 62,853 | $0.0309 | 9.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
