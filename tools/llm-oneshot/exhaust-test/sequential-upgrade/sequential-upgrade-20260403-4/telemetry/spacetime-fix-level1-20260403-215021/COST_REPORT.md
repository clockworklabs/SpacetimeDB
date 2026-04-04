# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-03T21:50:21-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 16 |
| Total output tokens     | 5,866 |
| Total tokens            | 5,882 |
| Cache read tokens       | 545,083 |
| Cache creation tokens   | 28,231 |
| Total cost (USD)        | $0.3574 |
| Total API time          | 102.4s |
| API calls               | 14 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 159 | 20,668 | $0.0424 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 144 | 29,678 | $0.0133 | 4.8s |
| 3 | claude-sonnet-4-6 | 1 | 198 | 32,574 | $0.0226 | 7.9s |
| 4 | claude-sonnet-4-6 | 1 | 1,616 | 35,196 | $0.0391 | 28.6s |
| 5 | claude-sonnet-4-6 | 1 | 443 | 36,349 | $0.0245 | 5.3s |
| 6 | claude-sonnet-4-6 | 1 | 254 | 38,205 | $0.0177 | 3.8s |
| 7 | claude-sonnet-4-6 | 1 | 191 | 38,864 | $0.0159 | 3.8s |
| 8 | claude-sonnet-4-6 | 1 | 394 | 39,230 | $0.0198 | 5.1s |
| 9 | claude-sonnet-4-6 | 1 | 408 | 40,309 | $0.0215 | 8.1s |
| 10 | claude-sonnet-4-6 | 1 | 161 | 41,194 | $0.0177 | 2.4s |
| 11 | claude-sonnet-4-6 | 1 | 705 | 42,879 | $0.0485 | 15.8s |
| 12 | claude-sonnet-4-6 | 1 | 160 | 49,569 | $0.0203 | 2.8s |
| 13 | claude-sonnet-4-6 | 1 | 873 | 49,569 | $0.0326 | 7.3s |
| 14 | claude-sonnet-4-6 | 1 | 160 | 50,799 | $0.0214 | 2.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
