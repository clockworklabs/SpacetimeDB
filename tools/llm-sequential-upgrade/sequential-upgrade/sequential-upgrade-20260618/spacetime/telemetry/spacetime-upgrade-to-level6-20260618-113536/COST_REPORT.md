# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 6
**Date:** 2026-06-18
**Started:** 2026-06-18T11:35:36-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,414 |
| Total output tokens     | 15,214 |
| Total tokens            | 16,628 |
| Cache read tokens       | 1,843,287 |
| Cache creation tokens   | 37,681 |
| Total cost (USD)        | $0.9238 |
| Total API time          | 245.7s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,386 | 19 | 0 | $0.0015 | 1.4s |
| 2 | claude-sonnet-4-6 | 1 | 204 | 35,680 | $0.0150 | 4.8s |
| 3 | claude-sonnet-4-6 | 1 | 255 | 36,020 | $0.0319 | 5.3s |
| 4 | claude-sonnet-4-6 | 1 | 5,823 | 40,630 | $0.1567 | 92.8s |
| 5 | claude-sonnet-4-6 | 1 | 754 | 55,886 | $0.0501 | 9.7s |
| 6 | claude-sonnet-4-6 | 1 | 295 | 61,767 | $0.0266 | 4.5s |
| 7 | claude-sonnet-4-6 | 1 | 286 | 62,744 | $0.0247 | 4.9s |
| 8 | claude-sonnet-4-6 | 1 | 399 | 63,163 | $0.0265 | 6.3s |
| 9 | claude-sonnet-4-6 | 1 | 1,201 | 63,573 | $0.0390 | 13.8s |
| 10 | claude-sonnet-4-6 | 1 | 210 | 64,077 | $0.0273 | 3.9s |
| 11 | claude-sonnet-4-6 | 1 | 289 | 65,383 | $0.0253 | 8.8s |
| 12 | claude-sonnet-4-6 | 1 | 281 | 66,930 | $0.0256 | 4.6s |
| 13 | claude-sonnet-4-6 | 1 | 331 | 67,288 | $0.0266 | 4.9s |
| 14 | claude-sonnet-4-6 | 1 | 263 | 67,669 | $0.0259 | 3.6s |
| 15 | claude-sonnet-4-6 | 1 | 276 | 68,100 | $0.0259 | 6.4s |
| 16 | claude-sonnet-4-6 | 1 | 361 | 68,463 | $0.0274 | 5.9s |
| 17 | claude-sonnet-4-6 | 1 | 427 | 68,839 | $0.0292 | 8.2s |
| 18 | claude-sonnet-4-6 | 1 | 314 | 69,399 | $0.0275 | 5.4s |
| 19 | claude-sonnet-4-6 | 1 | 492 | 69,926 | $0.0299 | 6.2s |
| 20 | claude-sonnet-4-6 | 1 | 1,222 | 70,340 | $0.0417 | 14.8s |
| 21 | claude-sonnet-4-6 | 1 | 166 | 70,932 | $0.0287 | 2.8s |
| 22 | claude-sonnet-4-6 | 1 | 230 | 74,562 | $0.0272 | 3.2s |
| 23 | claude-sonnet-4-6 | 1 | 152 | 75,225 | $0.0255 | 3.0s |
| 24 | claude-sonnet-4-6 | 1 | 245 | 75,392 | $0.0271 | 3.2s |
| 25 | claude-sonnet-4-6 | 1 | 176 | 75,601 | $0.0266 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 158 | 75,946 | $0.0263 | 3.9s |
| 27 | claude-sonnet-4-6 | 1 | 103 | 76,239 | $0.0261 | 2.7s |
| 28 | claude-sonnet-4-6 | 1 | 164 | 76,695 | $0.0259 | 3.7s |
| 29 | claude-sonnet-4-6 | 1 | 118 | 76,818 | $0.0260 | 3.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
