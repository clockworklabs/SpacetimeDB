# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 6
**Date:** 2026-06-16
**Started:** 2026-06-16T11:42:08-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,280 |
| Total output tokens     | 14,979 |
| Total tokens            | 17,259 |
| Cache read tokens       | 1,535,504 |
| Cache creation tokens   | 57,408 |
| Total cost (USD)        | $0.9028 |
| Total API time          | 220.6s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,256 | 18 | 0 | $0.0023 | 2.2s |
| 2 | claude-sonnet-4-6 | 3 | 327 | 20,501 | $0.0594 | 7.2s |
| 3 | claude-sonnet-4-6 | 1 | 244 | 33,379 | $0.0351 | 5.6s |
| 4 | claude-sonnet-4-6 | 1 | 5,871 | 42,986 | $0.1506 | 73.4s |
| 5 | claude-sonnet-4-6 | 1 | 875 | 56,232 | $0.0856 | 34.3s |
| 6 | claude-sonnet-4-6 | 1 | 226 | 71,068 | $0.0284 | 3.5s |
| 7 | claude-sonnet-4-6 | 1 | 597 | 72,061 | $0.0319 | 5.8s |
| 8 | claude-sonnet-4-6 | 1 | 977 | 72,405 | $0.0390 | 10.0s |
| 9 | claude-sonnet-4-6 | 1 | 268 | 73,101 | $0.0300 | 5.0s |
| 10 | claude-sonnet-4-6 | 1 | 349 | 74,177 | $0.0289 | 3.7s |
| 11 | claude-sonnet-4-6 | 1 | 560 | 74,544 | $0.0328 | 8.4s |
| 12 | claude-sonnet-4-6 | 1 | 645 | 75,091 | $0.0347 | 7.7s |
| 13 | claude-sonnet-4-6 | 1 | 418 | 75,750 | $0.0318 | 5.5s |
| 14 | claude-sonnet-4-6 | 1 | 511 | 76,494 | $0.0326 | 4.7s |
| 15 | claude-sonnet-4-6 | 1 | 472 | 77,011 | $0.0325 | 5.7s |
| 16 | claude-sonnet-4-6 | 1 | 866 | 77,621 | $0.0384 | 9.3s |
| 17 | claude-sonnet-4-6 | 1 | 986 | 78,192 | $0.0422 | 10.4s |
| 18 | claude-sonnet-4-6 | 1 | 155 | 79,256 | $0.0302 | 3.9s |
| 19 | claude-sonnet-4-6 | 1 | 153 | 80,341 | $0.0270 | 2.5s |
| 20 | claude-sonnet-4-6 | 1 | 113 | 80,514 | $0.0275 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 162 | 80,958 | $0.0278 | 3.3s |
| 22 | claude-sonnet-4-6 | 1 | 157 | 81,236 | $0.0280 | 3.3s |
| 23 | claude-sonnet-4-6 | 1 | 29 | 82,586 | $0.0261 | 2.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
