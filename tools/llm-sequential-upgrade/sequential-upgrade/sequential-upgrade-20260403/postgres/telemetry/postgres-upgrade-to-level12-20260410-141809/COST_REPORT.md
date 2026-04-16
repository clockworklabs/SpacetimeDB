# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T14:18:09-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 37 |
| Total output tokens     | 20,465 |
| Total tokens            | 20,502 |
| Cache read tokens       | 2,313,429 |
| Cache creation tokens   | 63,481 |
| Total cost (USD)        | $1.2392 |
| Total API time          | 321.7s |
| API calls               | 35 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 279 | 20,574 | $0.0421 | 5.7s |
| 2 | claude-sonnet-4-6 | 1 | 293 | 29,777 | $0.0207 | 5.5s |
| 3 | claude-sonnet-4-6 | 1 | 330 | 31,741 | $0.0287 | 5.4s |
| 4 | claude-sonnet-4-6 | 1 | 430 | 35,545 | $0.0360 | 8.0s |
| 5 | claude-sonnet-4-6 | 1 | 1,112 | 40,578 | $0.0534 | 21.2s |
| 6 | claude-sonnet-4-6 | 1 | 155 | 47,133 | $0.0315 | 4.2s |
| 7 | claude-sonnet-4-6 | 1 | 155 | 51,146 | $0.0293 | 3.8s |
| 8 | claude-sonnet-4-6 | 1 | 155 | 54,236 | $0.0243 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 425 | 55,751 | $0.0250 | 8.0s |
| 10 | claude-sonnet-4-6 | 1 | 155 | 56,249 | $0.0283 | 4.5s |
| 11 | claude-sonnet-4-6 | 1 | 155 | 60,235 | $0.0215 | 2.1s |
| 12 | claude-sonnet-4-6 | 1 | 156 | 62,037 | $0.0285 | 4.0s |
| 13 | claude-sonnet-4-6 | 1 | 123 | 65,720 | $0.0253 | 1.9s |
| 14 | claude-sonnet-4-6 | 1 | 571 | 65,720 | $0.0330 | 10.3s |
| 15 | claude-sonnet-4-6 | 1 | 359 | 66,968 | $0.0278 | 4.2s |
| 16 | claude-sonnet-4-6 | 1 | 193 | 67,582 | $0.0249 | 3.5s |
| 17 | claude-sonnet-4-6 | 1 | 272 | 68,047 | $0.0254 | 4.1s |
| 18 | claude-sonnet-4-6 | 1 | 193 | 68,660 | $0.0278 | 5.6s |
| 19 | claude-sonnet-4-6 | 1 | 255 | 69,813 | $0.0257 | 3.5s |
| 20 | claude-sonnet-4-6 | 1 | 298 | 70,048 | $0.0268 | 3.6s |
| 21 | claude-sonnet-4-6 | 1 | 640 | 70,409 | $0.0322 | 7.7s |
| 22 | claude-sonnet-4-6 | 1 | 700 | 70,794 | $0.0345 | 8.3s |
| 23 | claude-sonnet-4-6 | 1 | 755 | 71,521 | $0.0357 | 9.1s |
| 24 | claude-sonnet-4-6 | 1 | 1,516 | 72,308 | $0.0482 | 17.0s |
| 25 | claude-sonnet-4-6 | 1 | 193 | 73,325 | $0.0309 | 3.6s |
| 26 | claude-sonnet-4-6 | 1 | 293 | 74,928 | $0.0278 | 4.5s |
| 27 | claude-sonnet-4-6 | 1 | 167 | 75,163 | $0.0264 | 2.7s |
| 28 | claude-sonnet-4-6 | 1 | 229 | 75,529 | $0.0278 | 4.1s |
| 29 | claude-sonnet-4-6 | 1 | 285 | 75,993 | $0.0282 | 4.4s |
| 30 | claude-sonnet-4-6 | 1 | 7,897 | 76,303 | $0.1452 | 114.1s |
| 31 | claude-sonnet-4-6 | 1 | 855 | 88,588 | $0.0762 | 14.2s |
| 32 | claude-sonnet-4-6 | 1 | 188 | 99,462 | $0.0338 | 2.7s |
| 33 | claude-sonnet-4-6 | 1 | 245 | 99,774 | $0.0349 | 3.7s |
| 34 | claude-sonnet-4-6 | 1 | 247 | 100,621 | $0.0346 | 4.3s |
| 35 | claude-sonnet-4-6 | 1 | 191 | 101,151 | $0.0367 | 8.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
