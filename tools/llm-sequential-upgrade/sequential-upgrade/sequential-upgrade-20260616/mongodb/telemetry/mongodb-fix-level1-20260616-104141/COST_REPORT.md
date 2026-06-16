# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 1
**Date:** 2026-06-16
**Started:** 2026-06-16T10:41:41-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 817 |
| Total output tokens     | 3,416 |
| Total tokens            | 4,233 |
| Cache read tokens       | 385,213 |
| Cache creation tokens   | 18,374 |
| Total cost (USD)        | $0.2364 |
| Total API time          | 75.4s |
| API calls               | 12 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 804 | 14 | 0 | $0.0009 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 176 | 20,501 | $0.0506 | 4.2s |
| 3 | claude-sonnet-4-6 | 1 | 163 | 31,653 | $0.0133 | 4.5s |
| 4 | claude-sonnet-4-6 | 1 | 784 | 32,014 | $0.0344 | 16.8s |
| 5 | claude-sonnet-4-6 | 1 | 448 | 35,501 | $0.0208 | 6.1s |
| 6 | claude-sonnet-4-6 | 1 | 682 | 36,403 | $0.0233 | 12.5s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 36,969 | $0.0164 | 4.2s |
| 8 | claude-sonnet-4-6 | 1 | 116 | 38,028 | $0.0139 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 186 | 38,235 | $0.0148 | 4.3s |
| 10 | claude-sonnet-4-6 | 1 | 183 | 38,383 | $0.0151 | 4.1s |
| 11 | claude-sonnet-4-6 | 1 | 132 | 38,596 | $0.0148 | 4.3s |
| 12 | claude-sonnet-4-6 | 1 | 371 | 38,930 | $0.0181 | 9.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
