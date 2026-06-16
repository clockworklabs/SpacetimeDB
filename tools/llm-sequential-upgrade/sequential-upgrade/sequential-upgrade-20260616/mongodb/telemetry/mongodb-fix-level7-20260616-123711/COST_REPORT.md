# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 7
**Date:** 2026-06-16
**Started:** 2026-06-16T12:37:11-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 832 |
| Total output tokens     | 41,950 |
| Total tokens            | 42,782 |
| Cache read tokens       | 2,194,674 |
| Cache creation tokens   | 81,216 |
| Total cost (USD)        | $1.5930 |
| Total API time          | 716.4s |
| API calls               | 27 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 804 | 14 | 0 | $0.0009 | 1.0s |
| 2 | claude-sonnet-4-6 | 3 | 176 | 20,501 | $0.0510 | 4.2s |
| 3 | claude-sonnet-4-6 | 1 | 266 | 31,746 | $0.0160 | 4.5s |
| 4 | claude-sonnet-4-6 | 1 | 15,404 | 32,400 | $0.3167 | 240.6s |
| 5 | claude-sonnet-4-6 | 1 | 11,348 | 52,649 | $0.2484 | 200.1s |
| 6 | claude-sonnet-4-6 | 1 | 5,846 | 69,286 | $0.1520 | 105.7s |
| 7 | claude-sonnet-4-6 | 1 | 2,795 | 80,884 | $0.0887 | 43.4s |
| 8 | claude-sonnet-4-6 | 1 | 375 | 86,882 | $0.0426 | 5.9s |
| 9 | claude-sonnet-4-6 | 1 | 354 | 89,795 | $0.0341 | 4.3s |
| 10 | claude-sonnet-4-6 | 1 | 518 | 90,288 | $0.0366 | 5.4s |
| 11 | claude-sonnet-4-6 | 1 | 240 | 90,741 | $0.0331 | 4.8s |
| 12 | claude-sonnet-4-6 | 1 | 435 | 91,358 | $0.0353 | 5.6s |
| 13 | claude-sonnet-4-6 | 1 | 414 | 91,716 | $0.0361 | 5.2s |
| 14 | claude-sonnet-4-6 | 1 | 306 | 92,349 | $0.0342 | 4.5s |
| 15 | claude-sonnet-4-6 | 1 | 631 | 92,862 | $0.0388 | 8.3s |
| 16 | claude-sonnet-4-6 | 1 | 277 | 93,267 | $0.0349 | 5.4s |
| 17 | claude-sonnet-4-6 | 1 | 323 | 93,997 | $0.0345 | 6.7s |
| 18 | claude-sonnet-4-6 | 1 | 139 | 94,868 | $0.0315 | 3.6s |
| 19 | claude-sonnet-4-6 | 1 | 184 | 95,126 | $0.0324 | 7.6s |
| 20 | claude-sonnet-4-6 | 1 | 111 | 95,431 | $0.0316 | 3.3s |
| 21 | claude-sonnet-4-6 | 1 | 208 | 95,783 | $0.0334 | 5.2s |
| 22 | claude-sonnet-4-6 | 1 | 293 | 97,424 | $0.0485 | 8.0s |
| 23 | claude-sonnet-4-6 | 1 | 109 | 101,399 | $0.0338 | 2.9s |
| 24 | claude-sonnet-4-6 | 1 | 232 | 102,935 | $0.0353 | 5.1s |
| 25 | claude-sonnet-4-6 | 1 | 123 | 103,188 | $0.0347 | 4.1s |
| 26 | claude-sonnet-4-6 | 1 | 129 | 103,689 | $0.0346 | 2.8s |
| 27 | claude-sonnet-4-6 | 1 | 700 | 104,110 | $0.0433 | 18.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
