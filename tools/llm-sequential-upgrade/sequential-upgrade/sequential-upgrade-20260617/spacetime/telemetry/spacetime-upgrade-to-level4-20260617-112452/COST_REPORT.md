# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 4
**Date:** 2026-06-17
**Started:** 2026-06-17T11:24:52-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,336 |
| Total output tokens     | 8,432 |
| Total tokens            | 10,768 |
| Cache read tokens       | 1,550,289 |
| Cache creation tokens   | 47,456 |
| Total cost (USD)        | $0.8785 |
| Total API time          | 141.4s |
| API calls               | 27 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,308 | 19 | 0 | $0.0024 | 1.1s |
| 2 | claude-sonnet-4-6 | 3 | 321 | 20,621 | $0.1049 | 7.3s |
| 3 | claude-sonnet-4-6 | 1 | 259 | 36,261 | $0.0461 | 6.0s |
| 4 | claude-sonnet-4-6 | 1 | 598 | 41,476 | $0.1107 | 12.7s |
| 5 | claude-sonnet-4-6 | 1 | 761 | 56,350 | $0.0340 | 10.7s |
| 6 | claude-sonnet-4-6 | 1 | 248 | 57,303 | $0.0268 | 3.9s |
| 7 | claude-sonnet-4-6 | 1 | 520 | 58,287 | $0.0275 | 6.9s |
| 8 | claude-sonnet-4-6 | 1 | 177 | 58,659 | $0.0240 | 3.4s |
| 9 | claude-sonnet-4-6 | 1 | 184 | 59,284 | $0.0218 | 2.7s |
| 10 | claude-sonnet-4-6 | 1 | 290 | 59,496 | $0.0264 | 4.4s |
| 11 | claude-sonnet-4-6 | 1 | 186 | 60,201 | $0.0264 | 3.6s |
| 12 | claude-sonnet-4-6 | 1 | 239 | 61,643 | $0.0238 | 4.2s |
| 13 | claude-sonnet-4-6 | 1 | 270 | 61,934 | $0.0248 | 4.8s |
| 14 | claude-sonnet-4-6 | 1 | 847 | 62,292 | $0.0336 | 10.5s |
| 15 | claude-sonnet-4-6 | 1 | 234 | 62,662 | $0.0280 | 5.3s |
| 16 | claude-sonnet-4-6 | 1 | 225 | 63,609 | $0.0245 | 4.3s |
| 17 | claude-sonnet-4-6 | 1 | 299 | 63,943 | $0.0262 | 6.7s |
| 18 | claude-sonnet-4-6 | 1 | 270 | 64,367 | $0.0258 | 4.9s |
| 19 | claude-sonnet-4-6 | 1 | 353 | 64,766 | $0.0269 | 4.5s |
| 20 | claude-sonnet-4-6 | 1 | 842 | 65,136 | $0.0349 | 10.0s |
| 21 | claude-sonnet-4-6 | 1 | 260 | 65,589 | $0.0292 | 4.4s |
| 22 | claude-sonnet-4-6 | 1 | 188 | 66,531 | $0.0249 | 3.9s |
| 23 | claude-sonnet-4-6 | 1 | 176 | 67,266 | $0.0251 | 2.4s |
| 24 | claude-sonnet-4-6 | 1 | 181 | 67,650 | $0.0242 | 2.7s |
| 25 | claude-sonnet-4-6 | 1 | 305 | 67,845 | $0.0278 | 4.9s |
| 26 | claude-sonnet-4-6 | 1 | 160 | 68,324 | $0.0257 | 2.9s |
| 27 | claude-sonnet-4-6 | 1 | 20 | 68,794 | $0.0220 | 2.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
