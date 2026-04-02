# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-04-02
**Started:** 2026-04-02T15:39:29-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 39 |
| Total output tokens     | 65,509 |
| Total tokens            | 65,548 |
| Cache read tokens       | 2,217,037 |
| Cache creation tokens   | 53,466 |
| Total cost (USD)        | $1.8484 |
| Total API time          | 914.4s |
| API calls               | 37 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 196 | 20,668 | $0.0658 | 4.2s |
| 2 | claude-sonnet-4-6 | 1 | 32,000 | 35,767 | $0.4976 | 485.9s |
| 3 | claude-sonnet-4-6 | 1 | 226 | 38,061 | $0.0154 | 6.0s |
| 4 | claude-sonnet-4-6 | 1 | 209 | 38,478 | $0.0154 | 3.2s |
| 5 | claude-sonnet-4-6 | 1 | 239 | 38,674 | $0.0163 | 3.0s |
| 6 | claude-sonnet-4-6 | 1 | 1,521 | 38,674 | $0.0368 | 19.9s |
| 7 | claude-sonnet-4-6 | 1 | 4,768 | 39,317 | $0.0894 | 45.3s |
| 8 | claude-sonnet-4-6 | 1 | 155 | 40,937 | $0.0329 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 540 | 46,012 | $0.0236 | 8.4s |
| 10 | claude-sonnet-4-6 | 1 | 315 | 46,467 | $0.0228 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 281 | 47,578 | $0.0201 | 5.6s |
| 12 | claude-sonnet-4-6 | 1 | 264 | 48,012 | $0.0199 | 6.0s |
| 13 | claude-sonnet-4-6 | 1 | 359 | 48,412 | $0.0213 | 6.9s |
| 14 | claude-sonnet-4-6 | 1 | 2,020 | 49,235 | $0.0476 | 33.2s |
| 15 | claude-sonnet-4-6 | 1 | 193 | 49,913 | $0.0258 | 3.3s |
| 16 | claude-sonnet-4-6 | 1 | 739 | 52,032 | $0.0289 | 16.5s |
| 17 | claude-sonnet-4-6 | 1 | 234 | 52,613 | $0.0282 | 4.0s |
| 18 | claude-sonnet-4-6 | 1 | 4,839 | 54,983 | $0.0903 | 51.6s |
| 19 | claude-sonnet-4-6 | 1 | 227 | 60,256 | $0.0227 | 4.8s |
| 20 | claude-sonnet-4-6 | 1 | 218 | 66,374 | $0.0247 | 5.5s |
| 21 | claude-sonnet-4-6 | 1 | 399 | 66,766 | $0.0270 | 7.7s |
| 22 | claude-sonnet-4-6 | 1 | 202 | 67,026 | $0.0250 | 2.8s |
| 23 | claude-sonnet-4-6 | 1 | 338 | 67,026 | $0.0281 | 4.0s |
| 24 | claude-sonnet-4-6 | 1 | 257 | 67,814 | $0.0258 | 3.4s |
| 25 | claude-sonnet-4-6 | 1 | 395 | 68,245 | $0.0288 | 5.8s |
| 26 | claude-sonnet-4-6 | 1 | 8,123 | 68,873 | $0.1443 | 87.0s |
| 27 | claude-sonnet-4-6 | 1 | 4,468 | 69,362 | $0.1186 | 50.4s |
| 28 | claude-sonnet-4-6 | 1 | 168 | 77,579 | $0.0429 | 3.3s |
| 29 | claude-sonnet-4-6 | 1 | 218 | 82,141 | $0.0294 | 3.5s |
| 30 | claude-sonnet-4-6 | 1 | 175 | 82,544 | $0.0284 | 3.9s |
| 31 | claude-sonnet-4-6 | 1 | 224 | 82,804 | $0.0298 | 5.3s |
| 32 | claude-sonnet-4-6 | 1 | 179 | 83,233 | $0.0289 | 2.7s |
| 33 | claude-sonnet-4-6 | 1 | 176 | 83,571 | $0.0285 | 2.9s |
| 34 | claude-sonnet-4-6 | 1 | 226 | 83,768 | $0.0303 | 3.2s |
| 35 | claude-sonnet-4-6 | 1 | 98 | 84,243 | $0.0278 | 2.5s |
| 36 | claude-sonnet-4-6 | 1 | 104 | 84,632 | $0.0277 | 2.3s |
| 37 | claude-sonnet-4-6 | 1 | 216 | 84,947 | $0.0315 | 2.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
