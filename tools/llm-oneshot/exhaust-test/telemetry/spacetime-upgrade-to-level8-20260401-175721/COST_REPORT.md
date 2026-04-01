# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 8
**Date:** 2026-04-01
**Started:** 2026-04-01T17:57:21-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 35 |
| Total output tokens     | 19,938 |
| Total tokens            | 19,973 |
| Cache read tokens       | 2,315,614 |
| Cache creation tokens   | 55,366 |
| Total cost (USD)        | $1.2015 |
| Total API time          | 352.3s |
| API calls               | 35 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 274 | 35,553 | $0.0301 | 6.4s |
| 2 | claude-sonnet-4-6 | 1 | 102 | 31,799 | $0.0295 | 2.7s |
| 3 | claude-sonnet-4-6 | 1 | 131 | 43,123 | $0.0398 | 4.9s |
| 4 | claude-sonnet-4-6 | 1 | 243 | 37,791 | $0.0227 | 3.3s |
| 5 | claude-sonnet-4-6 | 1 | 148 | 49,762 | $0.0288 | 5.1s |
| 6 | claude-sonnet-4-6 | 1 | 148 | 52,881 | $0.0308 | 5.0s |
| 7 | claude-sonnet-4-6 | 1 | 277 | 45,956 | $0.0534 | 5.2s |
| 8 | claude-sonnet-4-6 | 1 | 148 | 59,082 | $0.0360 | 5.5s |
| 9 | claude-sonnet-4-6 | 1 | 1,234 | 63,350 | $0.0400 | 19.7s |
| 10 | claude-sonnet-4-6 | 1 | 589 | 64,017 | $0.0328 | 8.6s |
| 11 | claude-sonnet-4-6 | 1 | 205 | 65,281 | $0.0253 | 4.5s |
| 12 | claude-sonnet-4-6 | 1 | 207 | 65,974 | $0.0238 | 4.6s |
| 13 | claude-sonnet-4-6 | 1 | 1,207 | 66,221 | $0.0391 | 13.6s |
| 14 | claude-sonnet-4-6 | 1 | 205 | 66,532 | $0.0279 | 4.7s |
| 15 | claude-sonnet-4-6 | 1 | 268 | 68,822 | $0.0260 | 5.7s |
| 16 | claude-sonnet-4-6 | 1 | 707 | 70,109 | $0.0331 | 15.1s |
| 17 | claude-sonnet-4-6 | 1 | 270 | 70,489 | $0.0300 | 6.2s |
| 18 | claude-sonnet-4-6 | 1 | 203 | 71,781 | $0.0260 | 10.5s |
| 19 | claude-sonnet-4-6 | 1 | 8,000 | 64,037 | $0.1513 | 116.6s |
| 20 | claude-sonnet-4-6 | 1 | 222 | 71,781 | $0.0280 | 7.6s |
| 21 | claude-sonnet-4-6 | 1 | 442 | 72,623 | $0.0296 | 7.0s |
| 22 | claude-sonnet-4-6 | 1 | 444 | 72,925 | $0.0305 | 8.7s |
| 23 | claude-sonnet-4-6 | 1 | 241 | 73,447 | $0.0276 | 4.5s |
| 24 | claude-sonnet-4-6 | 1 | 1,234 | 73,971 | $0.0419 | 15.1s |
| 25 | claude-sonnet-4-6 | 1 | 162 | 74,292 | $0.0296 | 3.8s |
| 26 | claude-sonnet-4-6 | 1 | 148 | 76,690 | $0.0258 | 9.0s |
| 27 | claude-sonnet-4-6 | 1 | 148 | 76,844 | $0.0264 | 4.2s |
| 28 | claude-sonnet-4-6 | 1 | 1,079 | 77,157 | $0.0408 | 11.2s |
| 29 | claude-sonnet-4-6 | 1 | 205 | 77,536 | $0.0307 | 7.4s |
| 30 | claude-sonnet-4-6 | 1 | 140 | 78,695 | $0.0266 | 3.2s |
| 31 | claude-sonnet-4-6 | 1 | 143 | 78,942 | $0.0264 | 3.1s |
| 32 | claude-sonnet-4-6 | 1 | 103 | 79,100 | $0.0267 | 4.1s |
| 33 | claude-sonnet-4-6 | 1 | 175 | 79,485 | $0.0270 | 3.5s |
| 34 | claude-sonnet-4-6 | 1 | 213 | 79,614 | $0.0283 | 3.3s |
| 35 | claude-sonnet-4-6 | 1 | 273 | 79,952 | $0.0290 | 8.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
