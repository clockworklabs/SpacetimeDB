# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-04-06
**Started:** 2026-04-06T16:22:10-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 188 |
| Total output tokens     | 23,175 |
| Total tokens            | 23,363 |
| Cache read tokens       | 2,045,687 |
| Cache creation tokens   | 58,576 |
| Total cost (USD)        | $1.1816 |
| Total API time          | 368.6s |
| API calls               | 30 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 311 | 20,510 | $0.0553 | 6.5s |
| 2 | claude-sonnet-4-6 | 157 | 245 | 32,358 | $0.0181 | 3.6s |
| 3 | claude-sonnet-4-6 | 1 | 455 | 33,501 | $0.0328 | 10.3s |
| 4 | claude-sonnet-4-6 | 1 | 9,350 | 37,749 | $0.1962 | 137.1s |
| 5 | claude-sonnet-4-6 | 1 | 1,834 | 49,643 | $0.0902 | 30.9s |
| 6 | claude-sonnet-4-6 | 1 | 677 | 64,250 | $0.0307 | 19.0s |
| 7 | claude-sonnet-4-6 | 1 | 769 | 64,585 | $0.0339 | 11.6s |
| 8 | claude-sonnet-4-6 | 1 | 244 | 65,377 | $0.0266 | 5.1s |
| 9 | claude-sonnet-4-6 | 1 | 288 | 66,262 | $0.0253 | 7.5s |
| 10 | claude-sonnet-4-6 | 1 | 863 | 66,933 | $0.0345 | 10.9s |
| 11 | claude-sonnet-4-6 | 1 | 216 | 67,328 | $0.0270 | 4.2s |
| 12 | claude-sonnet-4-6 | 1 | 310 | 69,375 | $0.0293 | 5.0s |
| 13 | claude-sonnet-4-6 | 1 | 508 | 70,397 | $0.0324 | 10.4s |
| 14 | claude-sonnet-4-6 | 1 | 141 | 72,819 | $0.0285 | 4.2s |
| 15 | claude-sonnet-4-6 | 1 | 1,263 | 74,033 | $0.0440 | 18.9s |
| 16 | claude-sonnet-4-6 | 1 | 559 | 74,795 | $0.0357 | 5.9s |
| 17 | claude-sonnet-4-6 | 1 | 235 | 76,096 | $0.0288 | 3.5s |
| 18 | claude-sonnet-4-6 | 1 | 277 | 76,747 | $0.0284 | 3.3s |
| 19 | claude-sonnet-4-6 | 1 | 481 | 77,074 | $0.0317 | 7.5s |
| 20 | claude-sonnet-4-6 | 1 | 439 | 77,443 | $0.0320 | 7.0s |
| 21 | claude-sonnet-4-6 | 1 | 1,020 | 78,016 | $0.0407 | 10.7s |
| 22 | claude-sonnet-4-6 | 1 | 579 | 78,547 | $0.0364 | 8.5s |
| 23 | claude-sonnet-4-6 | 1 | 743 | 79,659 | $0.0383 | 8.0s |
| 24 | claude-sonnet-4-6 | 1 | 216 | 80,530 | $0.0305 | 3.9s |
| 25 | claude-sonnet-4-6 | 1 | 173 | 80,530 | $0.0309 | 3.2s |
| 26 | claude-sonnet-4-6 | 1 | 166 | 81,623 | $0.0277 | 3.3s |
| 27 | claude-sonnet-4-6 | 1 | 200 | 81,814 | $0.0293 | 3.1s |
| 28 | claude-sonnet-4-6 | 1 | 122 | 82,282 | $0.0279 | 5.8s |
| 29 | claude-sonnet-4-6 | 1 | 214 | 82,638 | $0.0285 | 2.9s |
| 30 | claude-sonnet-4-6 | 1 | 277 | 82,773 | $0.0299 | 6.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
