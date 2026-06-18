# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-06-18
**Started:** 2026-06-18T11:59:42-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 891 |
| Total output tokens     | 18,163 |
| Total tokens            | 19,054 |
| Cache read tokens       | 1,731,801 |
| Cache creation tokens   | 66,777 |
| Total cost (USD)        | $1.0432 |
| Total API time          | 287.6s |
| API calls               | 20 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 870 | 14 | 0 | $0.0009 | 2.0s |
| 2 | claude-sonnet-4-6 | 3 | 285 | 20,621 | $0.0651 | 4.9s |
| 3 | claude-sonnet-4-6 | 1 | 283 | 35,201 | $0.0177 | 4.6s |
| 4 | claude-sonnet-4-6 | 1 | 5,804 | 42,596 | $0.1512 | 99.7s |
| 5 | claude-sonnet-4-6 | 1 | 231 | 56,280 | $0.0451 | 4.0s |
| 6 | claude-sonnet-4-6 | 1 | 865 | 62,887 | $0.0505 | 15.1s |
| 7 | claude-sonnet-4-6 | 1 | 181 | 70,456 | $0.0254 | 3.7s |
| 8 | claude-sonnet-4-6 | 1 | 245 | 71,278 | $0.0330 | 5.3s |
| 9 | claude-sonnet-4-6 | 1 | 153 | 86,537 | $0.0695 | 2.6s |
| 10 | claude-sonnet-4-6 | 1 | 152 | 98,039 | $0.0362 | 2.6s |
| 11 | claude-sonnet-4-6 | 1 | 178 | 111,134 | $0.0377 | 6.3s |
| 12 | claude-sonnet-4-6 | 1 | 3,340 | 112,104 | $0.0870 | 54.1s |
| 13 | claude-sonnet-4-6 | 1 | 4,911 | 112,980 | $0.1205 | 53.3s |
| 14 | claude-sonnet-4-6 | 1 | 316 | 116,439 | $0.0585 | 5.7s |
| 15 | claude-sonnet-4-6 | 1 | 180 | 121,469 | $0.0411 | 3.1s |
| 16 | claude-sonnet-4-6 | 1 | 161 | 121,984 | $0.0398 | 2.6s |
| 17 | claude-sonnet-4-6 | 1 | 92 | 122,642 | $0.0387 | 3.1s |
| 18 | claude-sonnet-4-6 | 1 | 104 | 122,895 | $0.0391 | 2.1s |
| 19 | claude-sonnet-4-6 | 1 | 130 | 123,071 | $0.0393 | 2.5s |
| 20 | claude-sonnet-4-6 | 1 | 538 | 123,188 | $0.0467 | 10.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
