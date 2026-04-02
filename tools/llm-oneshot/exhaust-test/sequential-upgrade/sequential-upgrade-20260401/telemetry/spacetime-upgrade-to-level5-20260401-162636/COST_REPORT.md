# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-04-01
**Started:** 2026-04-01T16:26:36-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 16 |
| Total output tokens     | 7,385 |
| Total tokens            | 7,401 |
| Cache read tokens       | 945,180 |
| Cache creation tokens   | 9,318 |
| Total cost (USD)        | $0.4293 |
| Total API time          | 101.3s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 765 | 56,676 | $0.0292 | 11.1s |
| 2 | claude-sonnet-4-6 | 1 | 366 | 56,866 | $0.0257 | 6.0s |
| 3 | claude-sonnet-4-6 | 1 | 1,188 | 65,783 | $0.0428 | 11.8s |
| 4 | claude-sonnet-4-6 | 1 | 196 | 57,711 | $0.0219 | 4.3s |
| 5 | claude-sonnet-4-6 | 1 | 255 | 67,194 | $0.0296 | 4.0s |
| 6 | claude-sonnet-4-6 | 1 | 287 | 35,082 | $0.0167 | 3.8s |
| 7 | claude-sonnet-4-6 | 1 | 201 | 31,321 | $0.0143 | 3.7s |
| 8 | claude-sonnet-4-6 | 1 | 352 | 58,157 | $0.0236 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 329 | 58,395 | $0.0241 | 4.9s |
| 10 | claude-sonnet-4-6 | 1 | 391 | 58,827 | $0.0250 | 5.0s |
| 11 | claude-sonnet-4-6 | 1 | 571 | 59,236 | $0.0281 | 8.7s |
| 12 | claude-sonnet-4-6 | 1 | 255 | 69,274 | $0.0275 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 171 | 70,056 | $0.0247 | 3.8s |
| 14 | claude-sonnet-4-6 | 1 | 176 | 70,353 | $0.0245 | 3.7s |
| 15 | claude-sonnet-4-6 | 1 | 113 | 70,542 | $0.0246 | 3.4s |
| 16 | claude-sonnet-4-6 | 1 | 1,769 | 59,707 | $0.0469 | 18.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
