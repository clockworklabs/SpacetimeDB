# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 12
**Date:** 2026-06-18
**Started:** 2026-06-18T15:37:53-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 4,054 |
| Total output tokens     | 18,599 |
| Total tokens            | 22,653 |
| Cache read tokens       | 2,690,027 |
| Cache creation tokens   | 61,876 |
| Total cost (USD)        | $1.3272 |
| Total API time          | 301.1s |
| API calls               | 34 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,392 | 16 | 0 | $0.0015 | 1.0s |
| 2 | claude-sonnet-4-6 | 3 | 337 | 20,621 | $0.0691 | 8.7s |
| 3 | claude-sonnet-4-6 | 2,628 | 214 | 36,053 | $0.0233 | 4.6s |
| 4 | claude-sonnet-4-6 | 1 | 7,931 | 48,156 | $0.2163 | 130.0s |
| 5 | claude-sonnet-4-6 | 1 | 330 | 70,270 | $0.0599 | 9.2s |
| 6 | claude-sonnet-4-6 | 1 | 269 | 79,289 | $0.0295 | 4.8s |
| 7 | claude-sonnet-4-6 | 1 | 306 | 79,743 | $0.0300 | 4.6s |
| 8 | claude-sonnet-4-6 | 1 | 306 | 80,136 | $0.0302 | 4.3s |
| 9 | claude-sonnet-4-6 | 1 | 290 | 80,547 | $0.0301 | 3.7s |
| 10 | claude-sonnet-4-6 | 1 | 625 | 80,958 | $0.0351 | 10.8s |
| 11 | claude-sonnet-4-6 | 1 | 490 | 81,353 | $0.0345 | 6.5s |
| 12 | claude-sonnet-4-6 | 1 | 265 | 82,097 | $0.0312 | 4.0s |
| 13 | claude-sonnet-4-6 | 1 | 514 | 82,786 | $0.0339 | 6.4s |
| 14 | claude-sonnet-4-6 | 1 | 214 | 83,151 | $0.0305 | 4.2s |
| 15 | claude-sonnet-4-6 | 1 | 287 | 83,765 | $0.0306 | 4.6s |
| 16 | claude-sonnet-4-6 | 1 | 545 | 84,079 | $0.0349 | 8.0s |
| 17 | claude-sonnet-4-6 | 1 | 333 | 84,466 | $0.0328 | 4.7s |
| 18 | claude-sonnet-4-6 | 1 | 878 | 85,111 | $0.0407 | 10.6s |
| 19 | claude-sonnet-4-6 | 1 | 768 | 85,643 | $0.0409 | 9.2s |
| 20 | claude-sonnet-4-6 | 1 | 437 | 86,621 | $0.0358 | 5.8s |
| 21 | claude-sonnet-4-6 | 1 | 311 | 87,489 | $0.0329 | 4.6s |
| 22 | claude-sonnet-4-6 | 1 | 623 | 88,026 | $0.0373 | 8.4s |
| 23 | claude-sonnet-4-6 | 1 | 166 | 88,437 | $0.0321 | 4.1s |
| 24 | claude-sonnet-4-6 | 1 | 152 | 89,259 | $0.0311 | 2.7s |
| 25 | claude-sonnet-4-6 | 1 | 153 | 89,794 | $0.0316 | 2.7s |
| 26 | claude-sonnet-4-6 | 1 | 153 | 90,436 | $0.0324 | 3.3s |
| 27 | claude-sonnet-4-6 | 1 | 153 | 91,230 | $0.0304 | 2.6s |
| 28 | claude-sonnet-4-6 | 1 | 512 | 91,427 | $0.0373 | 6.7s |
| 29 | claude-sonnet-4-6 | 1 | 200 | 92,019 | $0.0329 | 4.9s |
| 30 | claude-sonnet-4-6 | 1 | 186 | 92,631 | $0.0326 | 3.5s |
| 31 | claude-sonnet-4-6 | 1 | 165 | 93,169 | $0.0312 | 2.9s |
| 32 | claude-sonnet-4-6 | 1 | 190 | 93,373 | $0.0326 | 3.4s |
| 33 | claude-sonnet-4-6 | 1 | 158 | 93,841 | $0.0313 | 3.1s |
| 34 | claude-sonnet-4-6 | 1 | 122 | 94,051 | $0.0307 | 2.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
