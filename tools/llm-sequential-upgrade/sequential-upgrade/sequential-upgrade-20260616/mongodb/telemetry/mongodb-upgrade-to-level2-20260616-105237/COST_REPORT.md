# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 2
**Date:** 2026-06-16
**Started:** 2026-06-16T10:52:37-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,032 |
| Total output tokens     | 16,988 |
| Total tokens            | 19,020 |
| Cache read tokens       | 1,168,204 |
| Cache creation tokens   | 45,272 |
| Total cost (USD)        | $0.7770 |
| Total API time          | 254.9s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,009 | 14 | 0 | $0.0021 | 1.3s |
| 2 | claude-sonnet-4-6 | 3 | 272 | 20,501 | $0.0569 | 5.0s |
| 3 | claude-sonnet-4-6 | 1 | 117 | 32,935 | $0.0283 | 5.3s |
| 4 | claude-sonnet-4-6 | 1 | 4,356 | 37,378 | $0.0983 | 61.4s |
| 5 | claude-sonnet-4-6 | 1 | 4,685 | 43,179 | $0.1160 | 71.8s |
| 6 | claude-sonnet-4-6 | 1 | 186 | 51,912 | $0.0364 | 3.7s |
| 7 | claude-sonnet-4-6 | 1 | 683 | 56,715 | $0.0284 | 7.8s |
| 8 | claude-sonnet-4-6 | 1 | 467 | 57,019 | $0.0275 | 6.6s |
| 9 | claude-sonnet-4-6 | 1 | 450 | 57,919 | $0.0263 | 9.6s |
| 10 | claude-sonnet-4-6 | 1 | 301 | 58,485 | $0.0242 | 3.4s |
| 11 | claude-sonnet-4-6 | 1 | 333 | 59,053 | $0.0242 | 6.6s |
| 12 | claude-sonnet-4-6 | 1 | 602 | 59,453 | $0.0285 | 7.5s |
| 13 | claude-sonnet-4-6 | 1 | 469 | 59,885 | $0.0276 | 5.8s |
| 14 | claude-sonnet-4-6 | 1 | 648 | 60,586 | $0.0300 | 6.3s |
| 15 | claude-sonnet-4-6 | 1 | 918 | 61,154 | $0.0349 | 9.7s |
| 16 | claude-sonnet-4-6 | 1 | 1,237 | 61,901 | $0.0413 | 13.8s |
| 17 | claude-sonnet-4-6 | 1 | 329 | 63,017 | $0.0289 | 7.4s |
| 18 | claude-sonnet-4-6 | 1 | 172 | 64,353 | $0.0234 | 3.7s |
| 19 | claude-sonnet-4-6 | 1 | 146 | 64,755 | $0.0234 | 4.0s |
| 20 | claude-sonnet-4-6 | 1 | 101 | 65,534 | $0.0221 | 2.5s |
| 21 | claude-sonnet-4-6 | 1 | 149 | 65,884 | $0.0227 | 2.6s |
| 22 | claude-sonnet-4-6 | 1 | 353 | 66,586 | $0.0258 | 9.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
