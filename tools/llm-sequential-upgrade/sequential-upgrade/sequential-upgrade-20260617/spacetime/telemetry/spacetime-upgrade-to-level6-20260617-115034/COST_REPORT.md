# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 6
**Date:** 2026-06-17
**Started:** 2026-06-17T11:50:34-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,452 |
| Total output tokens     | 19,909 |
| Total tokens            | 22,361 |
| Cache read tokens       | 2,483,575 |
| Cache creation tokens   | 64,302 |
| Total cost (USD)        | $1.4319 |
| Total API time          | 306.3s |
| API calls               | 34 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,417 | 18 | 0 | $0.0025 | 2.7s |
| 2 | claude-sonnet-4-6 | 3 | 555 | 20,621 | $0.1093 | 9.8s |
| 3 | claude-sonnet-4-6 | 1 | 260 | 36,412 | $0.0540 | 7.7s |
| 4 | claude-sonnet-4-6 | 1 | 7,278 | 42,934 | $0.2348 | 107.7s |
| 5 | claude-sonnet-4-6 | 1 | 1,050 | 61,719 | $0.0783 | 13.4s |
| 6 | claude-sonnet-4-6 | 1 | 596 | 69,055 | $0.0373 | 8.4s |
| 7 | claude-sonnet-4-6 | 1 | 560 | 70,328 | $0.0338 | 5.6s |
| 8 | claude-sonnet-4-6 | 1 | 870 | 71,048 | $0.0384 | 8.9s |
| 9 | claude-sonnet-4-6 | 1 | 198 | 71,713 | $0.0303 | 3.4s |
| 10 | claude-sonnet-4-6 | 1 | 286 | 72,688 | $0.0319 | 4.2s |
| 11 | claude-sonnet-4-6 | 1 | 140 | 74,734 | $0.0272 | 2.5s |
| 12 | claude-sonnet-4-6 | 1 | 157 | 75,173 | $0.0294 | 2.4s |
| 13 | claude-sonnet-4-6 | 1 | 352 | 76,782 | $0.0298 | 6.0s |
| 14 | claude-sonnet-4-6 | 1 | 231 | 77,035 | $0.0294 | 3.2s |
| 15 | claude-sonnet-4-6 | 1 | 1,114 | 77,506 | $0.0420 | 14.8s |
| 16 | claude-sonnet-4-6 | 1 | 435 | 77,837 | $0.0372 | 6.9s |
| 17 | claude-sonnet-4-6 | 1 | 222 | 79,051 | $0.0309 | 4.1s |
| 18 | claude-sonnet-4-6 | 1 | 277 | 79,685 | $0.0300 | 3.2s |
| 19 | claude-sonnet-4-6 | 1 | 510 | 80,007 | $0.0339 | 8.2s |
| 20 | claude-sonnet-4-6 | 1 | 411 | 80,384 | $0.0339 | 5.5s |
| 21 | claude-sonnet-4-6 | 1 | 299 | 80,994 | $0.0319 | 5.3s |
| 22 | claude-sonnet-4-6 | 1 | 528 | 81,505 | $0.0348 | 6.3s |
| 23 | claude-sonnet-4-6 | 1 | 613 | 81,904 | $0.0381 | 7.2s |
| 24 | claude-sonnet-4-6 | 1 | 426 | 82,631 | $0.0355 | 8.7s |
| 25 | claude-sonnet-4-6 | 1 | 731 | 83,344 | $0.0391 | 8.8s |
| 26 | claude-sonnet-4-6 | 1 | 235 | 84,701 | $0.0302 | 3.7s |
| 27 | claude-sonnet-4-6 | 1 | 429 | 84,904 | $0.0346 | 10.8s |
| 28 | claude-sonnet-4-6 | 1 | 238 | 85,912 | $0.0320 | 4.7s |
| 29 | claude-sonnet-4-6 | 1 | 203 | 86,351 | $0.0310 | 4.4s |
| 30 | claude-sonnet-4-6 | 1 | 167 | 86,689 | $0.0303 | 3.5s |
| 31 | claude-sonnet-4-6 | 1 | 171 | 86,992 | $0.0298 | 3.5s |
| 32 | claude-sonnet-4-6 | 1 | 201 | 87,177 | $0.0320 | 5.2s |
| 33 | claude-sonnet-4-6 | 1 | 128 | 87,649 | $0.0310 | 2.7s |
| 34 | claude-sonnet-4-6 | 1 | 20 | 88,110 | $0.0276 | 3.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
