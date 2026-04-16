# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 9
**Date:** 2026-04-07
**Started:** 2026-04-07T12:30:34-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 40 |
| Total output tokens     | 25,122 |
| Total tokens            | 25,162 |
| Cache read tokens       | 2,726,786 |
| Cache creation tokens   | 58,138 |
| Total cost (USD)        | $1.3869 |
| Total API time          | 324.2s |
| API calls               | 33 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 330 | 20,510 | $0.0571 | 7.2s |
| 2 | claude-haiku-4-5-20251001 | 6 | 173 | 31,106 | $0.0130 | 2.4s |
| 3 | claude-sonnet-4-6 | 1 | 8,001 | 58,096 | $0.1585 | 115.2s |
| 4 | claude-sonnet-4-6 | 1 | 338 | 63,719 | $0.0542 | 6.1s |
| 5 | claude-sonnet-4-6 | 1 | 574 | 71,732 | $0.0318 | 6.6s |
| 6 | claude-sonnet-4-6 | 1 | 251 | 72,186 | $0.0280 | 3.5s |
| 7 | claude-sonnet-4-6 | 1 | 802 | 72,876 | $0.0350 | 8.9s |
| 8 | claude-sonnet-4-6 | 1 | 824 | 73,169 | $0.0378 | 8.3s |
| 9 | claude-sonnet-4-6 | 1 | 2,421 | 74,087 | $0.0620 | 22.7s |
| 10 | claude-sonnet-4-6 | 1 | 251 | 75,008 | $0.0357 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 85 | 77,526 | $0.0256 | 2.3s |
| 12 | claude-sonnet-4-6 | 1 | 158 | 77,819 | $0.0262 | 3.0s |
| 13 | claude-sonnet-4-6 | 1 | 493 | 81,261 | $0.0517 | 9.1s |
| 14 | claude-sonnet-4-6 | 1 | 534 | 86,578 | $0.0380 | 7.8s |
| 15 | claude-sonnet-4-6 | 1 | 412 | 87,637 | $0.0346 | 4.6s |
| 16 | claude-sonnet-4-6 | 1 | 215 | 88,212 | $0.0317 | 3.2s |
| 17 | claude-sonnet-4-6 | 1 | 253 | 88,735 | $0.0316 | 3.2s |
| 18 | claude-sonnet-4-6 | 1 | 824 | 89,042 | $0.0404 | 8.8s |
| 19 | claude-sonnet-4-6 | 1 | 619 | 89,387 | $0.0395 | 7.8s |
| 20 | claude-sonnet-4-6 | 1 | 1,603 | 90,303 | $0.0538 | 15.1s |
| 21 | claude-sonnet-4-6 | 1 | 785 | 91,014 | $0.0454 | 9.5s |
| 22 | claude-sonnet-4-6 | 1 | 853 | 92,709 | $0.0448 | 8.9s |
| 23 | claude-sonnet-4-6 | 1 | 882 | 93,816 | $0.0449 | 8.9s |
| 24 | claude-sonnet-4-6 | 1 | 1,491 | 94,761 | $0.0544 | 14.7s |
| 25 | claude-sonnet-4-6 | 1 | 440 | 95,735 | $0.0413 | 5.6s |
| 26 | claude-sonnet-4-6 | 1 | 185 | 97,318 | $0.0340 | 3.0s |
| 27 | claude-sonnet-4-6 | 1 | 177 | 97,850 | $0.0336 | 3.0s |
| 28 | claude-sonnet-4-6 | 1 | 261 | 98,283 | $0.0352 | 4.2s |
| 29 | claude-sonnet-4-6 | 1 | 93 | 98,759 | $0.0322 | 2.2s |
| 30 | claude-sonnet-4-6 | 1 | 213 | 99,062 | $0.0333 | 3.5s |
| 31 | claude-sonnet-4-6 | 1 | 203 | 99,175 | $0.0337 | 4.5s |
| 32 | claude-sonnet-4-6 | 1 | 118 | 99,414 | $0.0334 | 2.9s |
| 33 | claude-sonnet-4-6 | 1 | 260 | 99,901 | $0.0344 | 3.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
