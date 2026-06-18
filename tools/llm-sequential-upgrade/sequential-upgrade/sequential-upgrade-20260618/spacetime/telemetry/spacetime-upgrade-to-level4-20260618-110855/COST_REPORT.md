# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 4
**Date:** 2026-06-18
**Started:** 2026-06-18T11:08:56-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,424 |
| Total output tokens     | 9,874 |
| Total tokens            | 11,298 |
| Cache read tokens       | 1,295,571 |
| Cache creation tokens   | 16,707 |
| Total cost (USD)        | $0.6007 |
| Total API time          | 146.5s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,403 | 17 | 0 | $0.0015 | 1.3s |
| 2 | claude-sonnet-4-6 | 1 | 204 | 35,776 | $0.0181 | 3.0s |
| 3 | claude-sonnet-4-6 | 1 | 1,052 | 53,451 | $0.0394 | 15.8s |
| 4 | claude-sonnet-4-6 | 1 | 764 | 55,485 | $0.0345 | 10.6s |
| 5 | claude-sonnet-4-6 | 1 | 583 | 57,177 | $0.0292 | 7.7s |
| 6 | claude-sonnet-4-6 | 1 | 150 | 58,772 | $0.0204 | 2.6s |
| 7 | claude-sonnet-4-6 | 1 | 181 | 58,921 | $0.0211 | 3.2s |
| 8 | claude-sonnet-4-6 | 1 | 269 | 59,106 | $0.0230 | 5.0s |
| 9 | claude-sonnet-4-6 | 1 | 185 | 59,434 | $0.0244 | 3.2s |
| 10 | claude-sonnet-4-6 | 1 | 140 | 60,446 | $0.0218 | 2.7s |
| 11 | claude-sonnet-4-6 | 1 | 1,357 | 60,865 | $0.0415 | 22.6s |
| 12 | claude-sonnet-4-6 | 1 | 2,811 | 61,623 | $0.0662 | 28.8s |
| 13 | claude-sonnet-4-6 | 1 | 315 | 63,099 | $0.0346 | 6.2s |
| 14 | claude-sonnet-4-6 | 1 | 225 | 66,010 | $0.0247 | 3.2s |
| 15 | claude-sonnet-4-6 | 1 | 323 | 66,425 | $0.0264 | 6.0s |
| 16 | claude-sonnet-4-6 | 1 | 306 | 66,849 | $0.0262 | 4.3s |
| 17 | claude-sonnet-4-6 | 1 | 163 | 67,272 | $0.0242 | 3.1s |
| 18 | claude-sonnet-4-6 | 1 | 145 | 67,678 | $0.0241 | 2.9s |
| 19 | claude-sonnet-4-6 | 1 | 175 | 68,525 | $0.0257 | 3.8s |
| 20 | claude-sonnet-4-6 | 1 | 158 | 69,205 | $0.0242 | 2.8s |
| 21 | claude-sonnet-4-6 | 1 | 167 | 69,497 | $0.0251 | 3.5s |
| 22 | claude-sonnet-4-6 | 1 | 184 | 69,955 | $0.0245 | 4.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
