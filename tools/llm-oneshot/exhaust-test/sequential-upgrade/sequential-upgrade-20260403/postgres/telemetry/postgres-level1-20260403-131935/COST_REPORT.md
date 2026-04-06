# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T13:19:35-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 36 |
| Total output tokens     | 23,532 |
| Total tokens            | 23,568 |
| Cache read tokens       | 1,780,181 |
| Cache creation tokens   | 39,036 |
| Total cost (USD)        | $1.0335 |
| Total API time          | 299.5s |
| API calls               | 34 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 278 | 20,668 | $0.0501 | 4.6s |
| 2 | claude-sonnet-4-6 | 1 | 2,186 | 31,270 | $0.0564 | 31.8s |
| 3 | claude-sonnet-4-6 | 1 | 256 | 37,720 | $0.0160 | 4.3s |
| 4 | claude-sonnet-4-6 | 1 | 242 | 38,225 | $0.0169 | 3.3s |
| 5 | claude-sonnet-4-6 | 1 | 164 | 38,696 | $0.0153 | 2.7s |
| 6 | claude-sonnet-4-6 | 1 | 236 | 39,029 | $0.0162 | 3.1s |
| 7 | claude-sonnet-4-6 | 1 | 771 | 39,281 | $0.0246 | 7.5s |
| 8 | claude-sonnet-4-6 | 1 | 4,130 | 39,611 | $0.0771 | 42.0s |
| 9 | claude-sonnet-4-6 | 1 | 345 | 40,474 | $0.0332 | 4.6s |
| 10 | claude-sonnet-4-6 | 1 | 252 | 44,696 | $0.0188 | 3.2s |
| 11 | claude-sonnet-4-6 | 1 | 295 | 45,131 | $0.0193 | 5.0s |
| 12 | claude-sonnet-4-6 | 1 | 276 | 45,476 | $0.0200 | 3.1s |
| 13 | claude-sonnet-4-6 | 1 | 219 | 46,058 | $0.0185 | 3.1s |
| 14 | claude-sonnet-4-6 | 1 | 3,501 | 46,424 | $0.0676 | 34.4s |
| 15 | claude-sonnet-4-6 | 1 | 5,408 | 46,735 | $0.1086 | 54.6s |
| 16 | claude-sonnet-4-6 | 1 | 233 | 50,328 | $0.0392 | 4.5s |
| 17 | claude-sonnet-4-6 | 1 | 167 | 55,828 | $0.0203 | 2.5s |
| 18 | claude-sonnet-4-6 | 1 | 84 | 58,635 | $0.0221 | 2.6s |
| 19 | claude-sonnet-4-6 | 1 | 652 | 59,504 | $0.0280 | 11.9s |
| 20 | claude-sonnet-4-6 | 1 | 900 | 59,605 | $0.0340 | 14.5s |
| 21 | claude-sonnet-4-6 | 1 | 291 | 60,300 | $0.0264 | 5.5s |
| 22 | claude-sonnet-4-6 | 1 | 174 | 61,343 | $0.0222 | 3.9s |
| 23 | claude-sonnet-4-6 | 1 | 273 | 61,660 | $0.0240 | 5.5s |
| 24 | claude-sonnet-4-6 | 1 | 178 | 63,412 | $0.0228 | 2.7s |
| 25 | claude-sonnet-4-6 | 1 | 189 | 63,705 | $0.0232 | 4.4s |
| 26 | claude-sonnet-4-6 | 1 | 182 | 64,043 | $0.0227 | 2.7s |
| 27 | claude-sonnet-4-6 | 1 | 174 | 64,250 | $0.0228 | 2.5s |
| 28 | claude-sonnet-4-6 | 1 | 172 | 64,485 | $0.0226 | 2.6s |
| 29 | claude-sonnet-4-6 | 1 | 245 | 64,677 | $0.0248 | 4.6s |
| 30 | claude-sonnet-4-6 | 1 | 119 | 65,140 | $0.0224 | 2.5s |
| 31 | claude-sonnet-4-6 | 1 | 101 | 65,578 | $0.0219 | 2.0s |
| 32 | claude-sonnet-4-6 | 1 | 106 | 65,889 | $0.0221 | 2.6s |
| 33 | claude-sonnet-4-6 | 1 | 502 | 66,093 | $0.0278 | 11.7s |
| 34 | claude-sonnet-4-6 | 1 | 231 | 66,212 | $0.0256 | 2.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
