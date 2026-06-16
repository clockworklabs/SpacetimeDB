# Cost Report

**App:** chat-app
**Backend:** mongodb
**Level:** 10
**Date:** 2026-06-16
**Started:** 2026-06-16T14:32:05-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,545 |
| Total output tokens     | 9,550 |
| Total tokens            | 12,095 |
| Cache read tokens       | 1,446,586 |
| Cache creation tokens   | 44,807 |
| Total cost (USD)        | $0.7477 |
| Total API time          | 151.7s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,522 | 14 | 0 | $0.0026 | 2.2s |
| 2 | claude-sonnet-4-6 | 3 | 580 | 20,501 | $0.0637 | 11.5s |
| 3 | claude-sonnet-4-6 | 1 | 2,308 | 45,547 | $0.1220 | 41.0s |
| 4 | claude-sonnet-4-6 | 1 | 846 | 65,204 | $0.0429 | 12.7s |
| 5 | claude-sonnet-4-6 | 1 | 335 | 68,042 | $0.0291 | 4.8s |
| 6 | claude-sonnet-4-6 | 1 | 379 | 69,006 | $0.0285 | 6.6s |
| 7 | claude-sonnet-4-6 | 1 | 338 | 69,558 | $0.0277 | 5.0s |
| 8 | claude-sonnet-4-6 | 1 | 312 | 70,036 | $0.0273 | 6.5s |
| 9 | claude-sonnet-4-6 | 1 | 361 | 70,473 | $0.0281 | 4.8s |
| 10 | claude-sonnet-4-6 | 1 | 687 | 70,884 | $0.0333 | 7.0s |
| 11 | claude-sonnet-4-6 | 1 | 799 | 71,344 | $0.0367 | 8.1s |
| 12 | claude-sonnet-4-6 | 1 | 796 | 72,229 | $0.0370 | 8.6s |
| 13 | claude-sonnet-4-6 | 1 | 161 | 73,127 | $0.0277 | 3.6s |
| 14 | claude-sonnet-4-6 | 1 | 151 | 74,315 | $0.0252 | 2.6s |
| 15 | claude-sonnet-4-6 | 1 | 481 | 74,489 | $0.0307 | 5.5s |
| 16 | claude-sonnet-4-6 | 1 | 163 | 74,794 | $0.0271 | 3.3s |
| 17 | claude-sonnet-4-6 | 1 | 175 | 75,374 | $0.0263 | 2.5s |
| 18 | claude-sonnet-4-6 | 1 | 124 | 75,654 | $0.0263 | 3.8s |
| 19 | claude-sonnet-4-6 | 1 | 174 | 76,119 | $0.0260 | 2.9s |
| 20 | claude-sonnet-4-6 | 1 | 91 | 76,275 | $0.0255 | 2.0s |
| 21 | claude-sonnet-4-6 | 1 | 172 | 76,615 | $0.0270 | 3.9s |
| 22 | claude-sonnet-4-6 | 1 | 103 | 77,000 | $0.0269 | 3.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
