# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 7
**Date:** 2026-04-02
**Started:** 2026-04-02T16:21:36-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 25 |
| Total output tokens     | 54,749 |
| Total tokens            | 54,774 |
| Cache read tokens       | 1,996,177 |
| Cache creation tokens   | 42,290 |
| Total cost (USD)        | $1.5788 |
| Total API time          | 622.8s |
| API calls               | 25 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 923 | 30,004 | $0.0424 | 14.1s |
| 2 | claude-sonnet-4-6 | 1 | 19,983 | 36,719 | $0.3119 | 230.5s |
| 3 | claude-sonnet-4-6 | 1 | 999 | 56,979 | $0.0330 | 10.6s |
| 4 | claude-sonnet-4-6 | 1 | 1,181 | 57,220 | $0.0402 | 12.6s |
| 5 | claude-sonnet-4-6 | 1 | 9,707 | 58,639 | $0.1680 | 90.9s |
| 6 | claude-sonnet-4-6 | 1 | 1,339 | 59,912 | $0.0748 | 14.2s |
| 7 | claude-sonnet-4-6 | 1 | 11,453 | 69,711 | $0.2005 | 119.8s |
| 8 | claude-sonnet-4-6 | 1 | 5,391 | 71,782 | $0.1457 | 53.1s |
| 9 | claude-sonnet-4-6 | 1 | 240 | 83,327 | $0.0492 | 5.2s |
| 10 | claude-sonnet-4-6 | 1 | 172 | 88,810 | $0.0303 | 2.9s |
| 11 | claude-sonnet-4-6 | 1 | 175 | 89,092 | $0.0301 | 2.8s |
| 12 | claude-sonnet-4-6 | 1 | 234 | 89,302 | $0.0319 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 423 | 89,721 | $0.0350 | 10.3s |
| 14 | claude-sonnet-4-6 | 1 | 183 | 91,192 | $0.0314 | 3.2s |
| 15 | claude-sonnet-4-6 | 1 | 183 | 91,531 | $0.0315 | 2.9s |
| 16 | claude-sonnet-4-6 | 1 | 185 | 91,874 | $0.0319 | 3.2s |
| 17 | claude-sonnet-4-6 | 1 | 174 | 92,284 | $0.0311 | 2.9s |
| 18 | claude-sonnet-4-6 | 1 | 172 | 92,504 | $0.0311 | 3.5s |
| 19 | claude-sonnet-4-6 | 1 | 253 | 92,696 | $0.0333 | 4.9s |
| 20 | claude-sonnet-4-6 | 1 | 137 | 93,142 | $0.0311 | 4.1s |
| 21 | claude-sonnet-4-6 | 1 | 113 | 93,606 | $0.0305 | 3.2s |
| 22 | claude-sonnet-4-6 | 1 | 179 | 93,803 | $0.0313 | 3.2s |
| 23 | claude-sonnet-4-6 | 1 | 127 | 93,929 | $0.0308 | 2.6s |
| 24 | claude-sonnet-4-6 | 1 | 585 | 94,129 | $0.0375 | 14.2s |
| 25 | claude-sonnet-4-6 | 1 | 238 | 94,269 | $0.0344 | 3.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
