# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 5
**Date:** 2026-06-16
**Started:** 2026-06-16T11:22:45-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,220 |
| Total output tokens     | 8,523 |
| Total tokens            | 10,743 |
| Cache read tokens       | 978,877 |
| Cache creation tokens   | 39,970 |
| Total cost (USD)        | $0.5735 |
| Total API time          | 125.9s |
| API calls               | 20 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,199 | 18 | 0 | $0.0023 | 2.2s |
| 2 | claude-sonnet-4-6 | 3 | 304 | 20,501 | $0.0583 | 4.8s |
| 3 | claude-sonnet-4-6 | 1 | 217 | 33,202 | $0.0333 | 5.1s |
| 4 | claude-sonnet-4-6 | 1 | 1,122 | 38,568 | $0.0664 | 20.3s |
| 5 | claude-sonnet-4-6 | 1 | 519 | 48,703 | $0.0270 | 6.8s |
| 6 | claude-sonnet-4-6 | 1 | 387 | 49,943 | $0.0232 | 4.6s |
| 7 | claude-sonnet-4-6 | 1 | 319 | 50,580 | $0.0222 | 4.5s |
| 8 | claude-sonnet-4-6 | 1 | 415 | 51,165 | $0.0231 | 5.7s |
| 9 | claude-sonnet-4-6 | 1 | 569 | 51,583 | $0.0259 | 11.5s |
| 10 | claude-sonnet-4-6 | 1 | 2,627 | 52,097 | $0.0575 | 26.7s |
| 11 | claude-sonnet-4-6 | 1 | 632 | 52,765 | $0.0355 | 7.8s |
| 12 | claude-sonnet-4-6 | 1 | 164 | 55,491 | $0.0219 | 2.9s |
| 13 | claude-sonnet-4-6 | 1 | 145 | 56,222 | $0.0220 | 2.8s |
| 14 | claude-sonnet-4-6 | 1 | 283 | 57,815 | $0.0267 | 4.0s |
| 15 | claude-sonnet-4-6 | 1 | 152 | 59,168 | $0.0214 | 2.9s |
| 16 | claude-sonnet-4-6 | 1 | 114 | 59,524 | $0.0212 | 2.7s |
| 17 | claude-sonnet-4-6 | 1 | 165 | 59,967 | $0.0210 | 3.1s |
| 18 | claude-sonnet-4-6 | 1 | 92 | 60,113 | $0.0210 | 2.5s |
| 19 | claude-sonnet-4-6 | 1 | 176 | 60,542 | $0.0223 | 2.8s |
| 20 | claude-sonnet-4-6 | 1 | 103 | 60,928 | $0.0211 | 2.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
