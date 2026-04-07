# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 4
**Date:** 2026-04-07
**Started:** 2026-04-07T09:15:39-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 35 |
| Total output tokens     | 10,033 |
| Total tokens            | 10,068 |
| Cache read tokens       | 1,899,513 |
| Cache creation tokens   | 34,211 |
| Total cost (USD)        | $0.8487 |
| Total API time          | 169.4s |
| API calls               | 31 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 207 | 20,510 | $0.0468 | 5.3s |
| 2 | claude-sonnet-4-6 | 1 | 758 | 43,436 | $0.0635 | 14.0s |
| 3 | claude-sonnet-4-6 | 1 | 196 | 54,648 | $0.0221 | 2.9s |
| 4 | claude-sonnet-4-6 | 1 | 552 | 54,648 | $0.0283 | 7.1s |
| 5 | claude-sonnet-4-6 | 1 | 724 | 55,616 | $0.0300 | 9.8s |
| 6 | claude-sonnet-4-6 | 1 | 338 | 56,279 | $0.0250 | 5.3s |
| 7 | claude-sonnet-4-6 | 1 | 895 | 57,095 | $0.0322 | 11.2s |
| 8 | claude-sonnet-4-6 | 1 | 196 | 57,525 | $0.0239 | 4.1s |
| 9 | claude-sonnet-4-6 | 1 | 387 | 58,512 | $0.0243 | 5.9s |
| 10 | claude-sonnet-4-6 | 1 | 535 | 58,750 | $0.0274 | 7.0s |
| 11 | claude-sonnet-4-6 | 1 | 348 | 59,229 | $0.0253 | 5.0s |
| 12 | claude-sonnet-4-6 | 1 | 1,146 | 59,856 | $0.0368 | 13.7s |
| 13 | claude-sonnet-4-6 | 1 | 176 | 60,296 | $0.0254 | 3.9s |
| 14 | claude-sonnet-4-6 | 1 | 196 | 63,006 | $0.0243 | 3.9s |
| 15 | claude-sonnet-4-6 | 1 | 172 | 63,671 | $0.0226 | 2.5s |
| 16 | claude-sonnet-4-6 | 1 | 192 | 63,909 | $0.0235 | 4.0s |
| 17 | claude-sonnet-4-6 | 1 | 179 | 64,306 | $0.0233 | 3.0s |
| 18 | claude-sonnet-4-6 | 1 | 186 | 64,987 | $0.0234 | 9.8s |
| 19 | claude-sonnet-4-6 | 1 | 187 | 65,287 | $0.0229 | 2.8s |
| 20 | claude-sonnet-4-6 | 1 | 208 | 65,421 | $0.0245 | 3.5s |
| 21 | claude-sonnet-4-6 | 1 | 210 | 65,896 | $0.0243 | 4.6s |
| 22 | claude-sonnet-4-6 | 1 | 209 | 66,258 | $0.0247 | 3.4s |
| 23 | claude-sonnet-4-6 | 1 | 310 | 66,713 | $0.0258 | 4.9s |
| 24 | claude-sonnet-4-6 | 1 | 125 | 67,017 | $0.0234 | 2.6s |
| 25 | claude-sonnet-4-6 | 1 | 322 | 67,556 | $0.0272 | 5.3s |
| 26 | claude-sonnet-4-6 | 1 | 146 | 69,039 | $0.0237 | 2.6s |
| 27 | claude-sonnet-4-6 | 1 | 151 | 69,243 | $0.0242 | 3.2s |
| 28 | claude-sonnet-4-6 | 1 | 321 | 69,541 | $0.0265 | 6.4s |
| 29 | claude-sonnet-4-6 | 1 | 207 | 70,098 | $0.0246 | 5.5s |
| 30 | claude-sonnet-4-6 | 1 | 233 | 70,212 | $0.0254 | 4.3s |
| 31 | claude-sonnet-4-6 | 3 | 21 | 70,953 | $0.0235 | 2.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
