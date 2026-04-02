# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-04-01
**Started:** 2026-04-01T19:51:16-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 26 |
| Total output tokens     | 6,195 |
| Total tokens            | 6,221 |
| Cache read tokens       | 1,156,544 |
| Cache creation tokens   | 26,897 |
| Total cost (USD)        | $0.5408 |
| Total API time          | 138.1s |
| API calls               | 24 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 208 | 20,619 | $0.0510 | 4.8s |
| 2 | claude-sonnet-4-6 | 1 | 206 | 35,743 | $0.0166 | 6.1s |
| 3 | claude-sonnet-4-6 | 1 | 329 | 36,496 | $0.0225 | 8.0s |
| 4 | claude-sonnet-4-6 | 1 | 217 | 38,257 | $0.0240 | 7.4s |
| 5 | claude-sonnet-4-6 | 1 | 147 | 42,707 | $0.0212 | 5.5s |
| 6 | claude-sonnet-4-6 | 1 | 147 | 44,361 | $0.0202 | 6.4s |
| 7 | claude-sonnet-4-6 | 1 | 348 | 46,317 | $0.0229 | 10.1s |
| 8 | claude-sonnet-4-6 | 1 | 323 | 48,220 | $0.0223 | 6.6s |
| 9 | claude-sonnet-4-6 | 1 | 147 | 49,006 | $0.0183 | 5.0s |
| 10 | claude-sonnet-4-6 | 1 | 643 | 49,374 | $0.0260 | 8.3s |
| 11 | claude-sonnet-4-6 | 1 | 327 | 49,795 | $0.0226 | 6.0s |
| 12 | claude-sonnet-4-6 | 1 | 157 | 50,536 | $0.0191 | 4.0s |
| 13 | claude-sonnet-4-6 | 1 | 309 | 50,961 | $0.0229 | 5.0s |
| 14 | claude-sonnet-4-6 | 1 | 289 | 51,763 | $0.0214 | 6.3s |
| 15 | claude-sonnet-4-6 | 1 | 416 | 52,170 | $0.0240 | 7.3s |
| 16 | claude-sonnet-4-6 | 1 | 199 | 53,492 | $0.0201 | 4.3s |
| 17 | claude-sonnet-4-6 | 1 | 139 | 53,788 | $0.0191 | 3.2s |
| 18 | claude-sonnet-4-6 | 1 | 139 | 54,029 | $0.0189 | 2.8s |
| 19 | claude-sonnet-4-6 | 1 | 209 | 54,186 | $0.0200 | 4.8s |
| 20 | claude-sonnet-4-6 | 1 | 157 | 54,343 | $0.0196 | 3.9s |
| 21 | claude-sonnet-4-6 | 1 | 93 | 54,594 | $0.0184 | 2.7s |
| 22 | claude-sonnet-4-6 | 1 | 157 | 54,771 | $0.0192 | 3.5s |
| 23 | claude-sonnet-4-6 | 1 | 692 | 54,877 | $0.0285 | 11.9s |
| 24 | claude-sonnet-4-6 | 1 | 197 | 56,139 | $0.0219 | 4.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
