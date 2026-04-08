# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 3
**Date:** 2026-04-06
**Started:** 2026-04-06T17:17:12-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 43 |
| Total output tokens     | 13,465 |
| Total tokens            | 13,508 |
| Cache read tokens       | 2,395,449 |
| Cache creation tokens   | 38,768 |
| Total cost (USD)        | $1.0661 |
| Total API time          | 197.4s |
| API calls               | 39 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 204 | 20,510 | $0.0464 | 3.7s |
| 2 | claude-sonnet-4-6 | 1 | 1,967 | 42,832 | $0.0789 | 33.2s |
| 3 | claude-sonnet-4-6 | 1 | 200 | 52,575 | $0.0263 | 3.6s |
| 4 | claude-sonnet-4-6 | 1 | 398 | 54,584 | $0.0233 | 5.8s |
| 5 | claude-sonnet-4-6 | 1 | 200 | 54,826 | $0.0214 | 3.5s |
| 6 | claude-sonnet-4-6 | 1 | 275 | 55,334 | $0.0216 | 4.2s |
| 7 | claude-sonnet-4-6 | 1 | 544 | 55,576 | $0.0263 | 5.6s |
| 8 | claude-sonnet-4-6 | 1 | 729 | 55,961 | $0.0301 | 7.6s |
| 9 | claude-sonnet-4-6 | 1 | 506 | 56,596 | $0.0276 | 6.1s |
| 10 | claude-sonnet-4-6 | 1 | 200 | 57,416 | $0.0225 | 4.0s |
| 11 | claude-sonnet-4-6 | 1 | 310 | 58,013 | $0.0230 | 3.6s |
| 12 | claude-sonnet-4-6 | 1 | 299 | 58,255 | $0.0235 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 374 | 58,656 | $0.0247 | 5.2s |
| 14 | claude-sonnet-4-6 | 1 | 343 | 59,046 | $0.0246 | 3.9s |
| 15 | claude-sonnet-4-6 | 1 | 397 | 59,511 | $0.0254 | 5.4s |
| 16 | claude-sonnet-4-6 | 1 | 443 | 59,945 | $0.0265 | 4.7s |
| 17 | claude-sonnet-4-6 | 1 | 610 | 60,433 | $0.0300 | 6.7s |
| 18 | claude-sonnet-4-6 | 1 | 723 | 61,149 | $0.0318 | 7.9s |
| 19 | claude-sonnet-4-6 | 1 | 175 | 61,850 | $0.0242 | 3.7s |
| 20 | claude-sonnet-4-6 | 1 | 526 | 62,664 | $0.0314 | 5.3s |
| 21 | claude-sonnet-4-6 | 1 | 200 | 63,913 | $0.0245 | 3.7s |
| 22 | claude-sonnet-4-6 | 1 | 182 | 64,530 | $0.0230 | 2.6s |
| 23 | claude-sonnet-4-6 | 1 | 199 | 64,772 | $0.0239 | 3.9s |
| 24 | claude-sonnet-4-6 | 1 | 250 | 65,179 | $0.0249 | 4.8s |
| 25 | claude-sonnet-4-6 | 1 | 173 | 65,603 | $0.0241 | 2.8s |
| 26 | claude-sonnet-4-6 | 1 | 187 | 66,078 | $0.0233 | 3.4s |
| 27 | claude-sonnet-4-6 | 1 | 773 | 66,265 | $0.0344 | 10.7s |
| 28 | claude-sonnet-4-6 | 1 | 239 | 67,052 | $0.0269 | 3.0s |
| 29 | claude-sonnet-4-6 | 1 | 175 | 67,915 | $0.0242 | 2.5s |
| 30 | claude-sonnet-4-6 | 1 | 184 | 68,245 | $0.0246 | 2.6s |
| 31 | claude-sonnet-4-6 | 1 | 165 | 68,620 | $0.0238 | 3.2s |
| 32 | claude-sonnet-4-6 | 1 | 140 | 68,822 | $0.0245 | 3.2s |
| 33 | claude-sonnet-4-6 | 1 | 217 | 69,277 | $0.0251 | 4.0s |
| 34 | claude-sonnet-4-6 | 1 | 213 | 69,571 | $0.0251 | 2.7s |
| 35 | claude-sonnet-4-6 | 1 | 219 | 69,859 | $0.0257 | 3.6s |
| 36 | claude-sonnet-4-6 | 1 | 97 | 70,255 | $0.0240 | 4.5s |
| 37 | claude-sonnet-4-6 | 1 | 218 | 70,634 | $0.0258 | 4.1s |
| 38 | claude-sonnet-4-6 | 1 | 198 | 71,373 | $0.0249 | 2.7s |
| 39 | claude-sonnet-4-6 | 3 | 13 | 71,754 | $0.0237 | 1.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
