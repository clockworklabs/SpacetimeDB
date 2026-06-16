# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 9
**Date:** 2026-06-16
**Started:** 2026-06-16T14:07:00-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,480 |
| Total output tokens     | 42,952 |
| Total tokens            | 45,432 |
| Cache read tokens       | 1,725,004 |
| Cache creation tokens   | 76,904 |
| Total cost (USD)        | $1.4525 |
| Total API time          | 499.2s |
| API calls               | 20 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,459 | 18 | 0 | $0.0025 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 743 | 20,501 | $0.0660 | 12.0s |
| 3 | claude-sonnet-4-6 | 1 | 13,291 | 42,845 | $0.2724 | 167.7s |
| 4 | claude-sonnet-4-6 | 1 | 8,559 | 58,888 | $0.1963 | 82.4s |
| 5 | claude-sonnet-4-6 | 1 | 16,311 | 72,278 | $0.2988 | 157.0s |
| 6 | claude-sonnet-4-6 | 1 | 237 | 80,936 | $0.0897 | 8.0s |
| 7 | claude-sonnet-4-6 | 1 | 151 | 97,445 | $0.0360 | 4.2s |
| 8 | claude-sonnet-4-6 | 1 | 151 | 98,657 | $0.0396 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 394 | 100,732 | $0.0400 | 6.8s |
| 10 | claude-sonnet-4-6 | 1 | 144 | 101,762 | $0.0360 | 3.9s |
| 11 | claude-sonnet-4-6 | 1 | 152 | 102,638 | $0.0339 | 3.3s |
| 12 | claude-sonnet-4-6 | 1 | 1,396 | 102,862 | $0.0534 | 14.0s |
| 13 | claude-sonnet-4-6 | 1 | 179 | 103,290 | $0.0394 | 4.4s |
| 14 | claude-sonnet-4-6 | 1 | 176 | 104,804 | $0.0352 | 4.2s |
| 15 | claude-sonnet-4-6 | 1 | 136 | 105,100 | $0.0353 | 5.1s |
| 16 | claude-sonnet-4-6 | 1 | 165 | 105,865 | $0.0350 | 3.6s |
| 17 | claude-sonnet-4-6 | 1 | 107 | 106,060 | $0.0345 | 3.6s |
| 18 | claude-sonnet-4-6 | 1 | 119 | 106,472 | $0.0345 | 3.5s |
| 19 | claude-sonnet-4-6 | 1 | 132 | 106,797 | $0.0351 | 3.7s |
| 20 | claude-sonnet-4-6 | 1 | 391 | 107,072 | $0.0389 | 6.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
