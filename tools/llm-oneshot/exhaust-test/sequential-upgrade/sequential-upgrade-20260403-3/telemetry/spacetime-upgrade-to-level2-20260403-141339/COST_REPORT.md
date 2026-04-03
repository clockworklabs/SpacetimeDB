# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-04-03
**Started:** 2026-04-03T14:13:39-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 29 |
| Total output tokens     | 23,845 |
| Total tokens            | 23,874 |
| Cache read tokens       | 2,054,066 |
| Cache creation tokens   | 53,631 |
| Total cost (USD)        | $1.1751 |
| Total API time          | 340.0s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 280 | 38,449 | $0.0377 | 5.7s |
| 2 | claude-sonnet-4-6 | 1 | 244 | 44,301 | $0.0301 | 6.2s |
| 3 | claude-sonnet-4-6 | 1 | 425 | 47,798 | $0.0574 | 7.6s |
| 4 | claude-sonnet-4-6 | 1 | 129 | 57,580 | $0.0254 | 5.0s |
| 5 | claude-sonnet-4-6 | 1 | 1,602 | 59,241 | $0.0476 | 26.5s |
| 6 | claude-sonnet-4-6 | 1 | 230 | 60,794 | $0.0278 | 3.9s |
| 7 | claude-sonnet-4-6 | 1 | 221 | 60,794 | $0.0287 | 6.2s |
| 8 | claude-sonnet-4-6 | 1 | 364 | 62,699 | $0.0255 | 6.8s |
| 9 | claude-sonnet-4-6 | 1 | 230 | 63,038 | $0.0242 | 4.1s |
| 10 | claude-sonnet-4-6 | 1 | 259 | 63,520 | $0.0240 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 962 | 63,792 | $0.0350 | 12.7s |
| 12 | claude-sonnet-4-6 | 1 | 656 | 64,169 | $0.0331 | 10.0s |
| 13 | claude-sonnet-4-6 | 1 | 300 | 65,230 | $0.0269 | 4.7s |
| 14 | claude-sonnet-4-6 | 1 | 262 | 65,987 | $0.0252 | 3.8s |
| 15 | claude-sonnet-4-6 | 1 | 230 | 66,386 | $0.0247 | 6.4s |
| 16 | claude-sonnet-4-6 | 1 | 4,409 | 67,019 | $0.0886 | 68.0s |
| 17 | claude-sonnet-4-6 | 1 | 1,084 | 67,648 | $0.0567 | 12.6s |
| 18 | claude-sonnet-4-6 | 1 | 162 | 73,021 | $0.0288 | 4.2s |
| 19 | claude-sonnet-4-6 | 1 | 2,632 | 74,204 | $0.0744 | 24.8s |
| 20 | claude-sonnet-4-6 | 1 | 253 | 81,142 | $0.0328 | 3.8s |
| 21 | claude-sonnet-4-6 | 1 | 428 | 82,391 | $0.0368 | 7.9s |
| 22 | claude-sonnet-4-6 | 1 | 230 | 83,908 | $0.0324 | 5.5s |
| 23 | claude-sonnet-4-6 | 1 | 7,052 | 84,916 | $0.1323 | 76.4s |
| 24 | claude-sonnet-4-6 | 1 | 159 | 85,188 | $0.0547 | 3.1s |
| 25 | claude-sonnet-4-6 | 1 | 230 | 93,394 | $0.0335 | 5.1s |
| 26 | claude-sonnet-4-6 | 1 | 176 | 93,946 | $0.0318 | 3.7s |
| 27 | claude-sonnet-4-6 | 1 | 170 | 94,218 | $0.0315 | 3.3s |
| 28 | claude-sonnet-4-6 | 1 | 238 | 94,412 | $0.0337 | 4.1s |
| 29 | claude-sonnet-4-6 | 1 | 228 | 94,881 | $0.0336 | 3.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
