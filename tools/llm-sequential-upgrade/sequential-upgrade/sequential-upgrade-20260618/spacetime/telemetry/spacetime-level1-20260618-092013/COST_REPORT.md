# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-06-18
**Started:** 2026-06-18T09:20:13-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,074 |
| Total output tokens     | 29,630 |
| Total tokens            | 31,704 |
| Cache read tokens       | 1,531,435 |
| Cache creation tokens   | 39,149 |
| Total cost (USD)        | $1.0527 |
| Total API time          | 411.6s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,053 | 14 | 0 | $0.0021 | 1.7s |
| 2 | claude-sonnet-4-6 | 1 | 435 | 52,971 | $0.0234 | 5.3s |
| 3 | claude-sonnet-4-6 | 1 | 811 | 53,243 | $0.0307 | 9.7s |
| 4 | claude-sonnet-4-6 | 1 | 1,897 | 53,916 | $0.0481 | 21.5s |
| 5 | claude-sonnet-4-6 | 1 | 171 | 54,830 | $0.0265 | 3.5s |
| 6 | claude-sonnet-4-6 | 1 | 697 | 57,152 | $0.0285 | 12.9s |
| 7 | claude-sonnet-4-6 | 1 | 188 | 57,384 | $0.0231 | 2.8s |
| 8 | claude-sonnet-4-6 | 1 | 250 | 58,496 | $0.0285 | 5.2s |
| 9 | claude-sonnet-4-6 | 1 | 342 | 60,407 | $0.0373 | 6.6s |
| 10 | claude-sonnet-4-6 | 1 | 274 | 64,156 | $0.0285 | 5.4s |
| 11 | claude-sonnet-4-6 | 1 | 12,038 | 65,515 | $0.2049 | 154.8s |
| 12 | claude-sonnet-4-6 | 1 | 362 | 66,767 | $0.0727 | 6.1s |
| 13 | claude-sonnet-4-6 | 1 | 5,994 | 79,359 | $0.1154 | 70.4s |
| 14 | claude-sonnet-4-6 | 1 | 4,484 | 79,819 | $0.1144 | 49.0s |
| 15 | claude-sonnet-4-6 | 1 | 169 | 86,010 | $0.0455 | 3.6s |
| 16 | claude-sonnet-4-6 | 1 | 165 | 90,592 | $0.0305 | 4.2s |
| 17 | claude-sonnet-4-6 | 1 | 394 | 90,814 | $0.0345 | 8.0s |
| 18 | claude-sonnet-4-6 | 1 | 169 | 91,167 | $0.0318 | 3.1s |
| 19 | claude-sonnet-4-6 | 1 | 166 | 91,680 | $0.0307 | 3.2s |
| 20 | claude-sonnet-4-6 | 1 | 104 | 91,867 | $0.0309 | 3.7s |
| 21 | claude-sonnet-4-6 | 1 | 196 | 92,557 | $0.0314 | 22.2s |
| 22 | claude-sonnet-4-6 | 1 | 310 | 92,733 | $0.0333 | 8.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
