# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-07
**Started:** 2026-04-07T12:08:19-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 16 |
| Total output tokens     | 4,560 |
| Total tokens            | 4,576 |
| Cache read tokens       | 499,317 |
| Cache creation tokens   | 21,171 |
| Total cost (USD)        | $0.2976 |
| Total API time          | 86.3s |
| API calls               | 14 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 163 | 20,510 | $0.0417 | 7.4s |
| 2 | claude-sonnet-4-6 | 1 | 463 | 29,842 | $0.0229 | 11.5s |
| 3 | claude-sonnet-4-6 | 1 | 159 | 33,119 | $0.0143 | 4.6s |
| 4 | claude-sonnet-4-6 | 1 | 547 | 33,657 | $0.0233 | 9.0s |
| 5 | claude-sonnet-4-6 | 1 | 821 | 34,985 | $0.0268 | 9.7s |
| 6 | claude-sonnet-4-6 | 1 | 310 | 36,049 | $0.0193 | 4.6s |
| 7 | claude-sonnet-4-6 | 1 | 490 | 37,084 | $0.0201 | 5.6s |
| 8 | claude-sonnet-4-6 | 1 | 175 | 37,504 | $0.0161 | 3.4s |
| 9 | claude-sonnet-4-6 | 1 | 342 | 38,085 | $0.0194 | 4.9s |
| 10 | claude-sonnet-4-6 | 1 | 167 | 38,832 | $0.0158 | 4.2s |
| 11 | claude-sonnet-4-6 | 1 | 119 | 39,265 | $0.0143 | 2.4s |
| 12 | claude-sonnet-4-6 | 1 | 100 | 39,601 | $0.0141 | 3.7s |
| 13 | claude-sonnet-4-6 | 1 | 97 | 40,208 | $0.0143 | 3.5s |
| 14 | claude-sonnet-4-6 | 1 | 607 | 40,576 | $0.0353 | 11.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
