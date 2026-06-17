# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 6
**Date:** 2026-06-17
**Started:** 2026-06-17T17:38:09-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,423 |
| Total output tokens     | 13,726 |
| Total tokens            | 15,149 |
| Cache read tokens       | 2,352,535 |
| Cache creation tokens   | 43,212 |
| Total cost (USD)        | $1.1723 |
| Total API time          | 204.0s |
| API calls               | 34 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,388 | 16 | 0 | $0.0015 | 1.1s |
| 2 | claude-sonnet-4-6 | 3 | 442 | 20,621 | $0.1016 | 6.2s |
| 3 | claude-sonnet-4-6 | 1 | 377 | 35,424 | $0.0400 | 5.5s |
| 4 | claude-sonnet-4-6 | 1 | 2,090 | 58,186 | $0.0767 | 28.4s |
| 5 | claude-sonnet-4-6 | 1 | 402 | 62,838 | $0.0382 | 6.1s |
| 6 | claude-sonnet-4-6 | 1 | 581 | 65,054 | $0.0314 | 6.6s |
| 7 | claude-sonnet-4-6 | 1 | 1,100 | 65,582 | $0.0403 | 11.6s |
| 8 | claude-sonnet-4-6 | 1 | 196 | 66,270 | $0.0307 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 231 | 67,576 | $0.0280 | 4.7s |
| 10 | claude-sonnet-4-6 | 1 | 307 | 68,281 | $0.0276 | 4.7s |
| 11 | claude-sonnet-4-6 | 1 | 356 | 69,488 | $0.0278 | 5.5s |
| 12 | claude-sonnet-4-6 | 1 | 497 | 69,749 | $0.0317 | 5.6s |
| 13 | claude-sonnet-4-6 | 1 | 363 | 70,306 | $0.0301 | 4.4s |
| 14 | claude-sonnet-4-6 | 1 | 230 | 70,905 | $0.0275 | 3.7s |
| 15 | claude-sonnet-4-6 | 1 | 450 | 71,370 | $0.0302 | 6.3s |
| 16 | claude-sonnet-4-6 | 1 | 413 | 71,702 | $0.0310 | 5.9s |
| 17 | claude-sonnet-4-6 | 1 | 283 | 72,254 | $0.0290 | 4.1s |
| 18 | claude-sonnet-4-6 | 1 | 417 | 72,769 | $0.0310 | 7.7s |
| 19 | claude-sonnet-4-6 | 1 | 413 | 73,253 | $0.0313 | 6.3s |
| 20 | claude-sonnet-4-6 | 1 | 534 | 73,772 | $0.0332 | 7.9s |
| 21 | claude-sonnet-4-6 | 1 | 154 | 74,287 | $0.0314 | 2.9s |
| 22 | claude-sonnet-4-6 | 1 | 1,067 | 75,427 | $0.0430 | 15.5s |
| 23 | claude-sonnet-4-6 | 1 | 169 | 76,152 | $0.0324 | 2.9s |
| 24 | claude-sonnet-4-6 | 1 | 180 | 78,126 | $0.0327 | 3.9s |
| 25 | claude-sonnet-4-6 | 1 | 154 | 79,214 | $0.0273 | 2.5s |
| 26 | claude-sonnet-4-6 | 1 | 261 | 79,410 | $0.0292 | 3.2s |
| 27 | claude-sonnet-4-6 | 1 | 760 | 80,274 | $0.0456 | 11.5s |
| 28 | claude-sonnet-4-6 | 1 | 183 | 81,953 | $0.0325 | 3.0s |
| 29 | claude-sonnet-4-6 | 1 | 425 | 82,815 | $0.0328 | 6.3s |
| 30 | claude-sonnet-4-6 | 1 | 169 | 83,081 | $0.0306 | 2.7s |
| 31 | claude-sonnet-4-6 | 1 | 171 | 83,608 | $0.0288 | 3.0s |
| 32 | claude-sonnet-4-6 | 1 | 192 | 83,795 | $0.0308 | 3.1s |
| 33 | claude-sonnet-4-6 | 1 | 122 | 84,265 | $0.0299 | 4.9s |
| 34 | claude-sonnet-4-6 | 1 | 21 | 84,728 | $0.0265 | 3.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
