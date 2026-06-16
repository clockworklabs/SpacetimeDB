# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 1
**Date:** 2026-06-16
**Started:** 2026-06-16T10:02:24-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,904 |
| Total output tokens     | 39,290 |
| Total tokens            | 41,194 |
| Cache read tokens       | 675,630 |
| Cache creation tokens   | 29,707 |
| Total cost (USD)        | $0.9052 |
| Total API time          | 487.3s |
| API calls               | 12 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,891 | 16 | 0 | $0.0020 | 4.9s |
| 2 | claude-sonnet-4-6 | 3 | 24,176 | 20,501 | $0.4146 | 316.0s |
| 3 | claude-sonnet-4-6 | 1 | 216 | 56,909 | $0.0221 | 6.8s |
| 4 | claude-sonnet-4-6 | 1 | 1,717 | 57,379 | $0.0438 | 18.3s |
| 5 | claude-sonnet-4-6 | 1 | 812 | 57,610 | $0.0391 | 8.8s |
| 6 | claude-sonnet-4-6 | 1 | 6,422 | 60,184 | $0.1183 | 59.5s |
| 7 | claude-sonnet-4-6 | 1 | 4,782 | 61,225 | $0.1150 | 47.8s |
| 8 | claude-sonnet-4-6 | 1 | 311 | 67,876 | $0.0433 | 4.7s |
| 9 | claude-sonnet-4-6 | 1 | 336 | 72,755 | $0.0287 | 4.6s |
| 10 | claude-sonnet-4-6 | 1 | 177 | 73,236 | $0.0265 | 3.2s |
| 11 | claude-sonnet-4-6 | 1 | 161 | 73,744 | $0.0263 | 8.9s |
| 12 | claude-sonnet-4-6 | 1 | 164 | 74,211 | $0.0255 | 3.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
