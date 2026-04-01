# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-04-01
**Started:** 2026-04-01T18:02:12-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 25 |
| Total output tokens     | 21,657 |
| Total tokens            | 21,682 |
| Cache read tokens       | 1,454,204 |
| Cache creation tokens   | 57,155 |
| Total cost (USD)        | $0.9755 |
| Total API time          | 357.4s |
| API calls               | 25 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 109 | 39,741 | $0.0140 | 3.8s |
| 2 | claude-sonnet-4-6 | 1 | 102 | 36,128 | $0.0179 | 3.4s |
| 3 | claude-sonnet-4-6 | 1 | 102 | 39,868 | $0.0179 | 4.1s |
| 4 | claude-sonnet-4-6 | 1 | 243 | 38,672 | $0.0232 | 6.3s |
| 5 | claude-sonnet-4-6 | 1 | 277 | 40,779 | $0.0313 | 5.6s |
| 6 | claude-sonnet-4-6 | 1 | 277 | 44,749 | $0.0422 | 5.5s |
| 7 | claude-sonnet-4-6 | 1 | 361 | 51,962 | $0.0251 | 8.2s |
| 8 | claude-sonnet-4-6 | 1 | 317 | 51,326 | $0.0435 | 5.9s |
| 9 | claude-sonnet-4-6 | 1 | 277 | 57,561 | $0.0419 | 5.0s |
| 10 | claude-sonnet-4-6 | 1 | 147 | 63,026 | $0.0433 | 4.9s |
| 11 | claude-sonnet-4-6 | 1 | 8,000 | 67,260 | $0.1852 | 118.3s |
| 12 | claude-sonnet-4-6 | 1 | 742 | 54,699 | $0.0295 | 14.2s |
| 13 | claude-sonnet-4-6 | 1 | 140 | 55,209 | $0.0216 | 4.6s |
| 14 | claude-sonnet-4-6 | 1 | 150 | 55,991 | $0.0196 | 3.7s |
| 15 | claude-sonnet-4-6 | 1 | 181 | 56,341 | $0.0205 | 2.7s |
| 16 | claude-sonnet-4-6 | 1 | 150 | 56,578 | $0.0202 | 2.9s |
| 17 | claude-sonnet-4-6 | 1 | 4,470 | 79,266 | $0.0908 | 65.8s |
| 18 | claude-sonnet-4-6 | 1 | 241 | 79,266 | $0.0442 | 3.5s |
| 19 | claude-sonnet-4-6 | 1 | 493 | 83,752 | $0.0336 | 7.7s |
| 20 | claude-sonnet-4-6 | 1 | 241 | 84,035 | $0.0310 | 4.7s |
| 21 | claude-sonnet-4-6 | 1 | 2,323 | 56,832 | $0.0526 | 39.4s |
| 22 | claude-sonnet-4-6 | 1 | 579 | 57,024 | $0.0348 | 11.0s |
| 23 | claude-sonnet-4-6 | 1 | 1,322 | 84,626 | $0.0463 | 13.9s |
| 24 | claude-sonnet-4-6 | 1 | 148 | 59,428 | $0.0225 | 6.7s |
| 25 | claude-sonnet-4-6 | 1 | 265 | 60,085 | $0.0227 | 5.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
