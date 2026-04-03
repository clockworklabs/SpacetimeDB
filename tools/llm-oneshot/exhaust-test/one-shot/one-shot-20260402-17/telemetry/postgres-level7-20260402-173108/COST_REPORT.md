# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 7
**Date:** 2026-04-02
**Started:** 2026-04-02T17:31:08-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 35 |
| Total output tokens     | 33,439 |
| Total tokens            | 33,474 |
| Cache read tokens       | 1,957,196 |
| Cache creation tokens   | 54,342 |
| Total cost (USD)        | $1.2926 |
| Total API time          | 438.0s |
| API calls               | 33 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 365 | 20,668 | $0.0540 | 8.4s |
| 2 | claude-sonnet-4-6 | 1 | 475 | 31,954 | $0.0405 | 9.5s |
| 3 | claude-sonnet-4-6 | 1 | 277 | 40,050 | $0.0171 | 4.8s |
| 4 | claude-sonnet-4-6 | 1 | 391 | 40,610 | $0.0187 | 8.9s |
| 5 | claude-sonnet-4-6 | 1 | 242 | 40,771 | $0.0177 | 6.2s |
| 6 | claude-sonnet-4-6 | 1 | 173 | 41,252 | $0.0162 | 5.1s |
| 7 | claude-sonnet-4-6 | 1 | 245 | 41,252 | $0.0183 | 4.8s |
| 8 | claude-sonnet-4-6 | 1 | 1,190 | 41,846 | $0.0317 | 14.3s |
| 9 | claude-sonnet-4-6 | 1 | 7,078 | 42,185 | $0.1236 | 68.1s |
| 10 | claude-sonnet-4-6 | 1 | 348 | 43,467 | $0.0459 | 7.2s |
| 11 | claude-sonnet-4-6 | 1 | 252 | 50,846 | $0.0207 | 5.7s |
| 12 | claude-sonnet-4-6 | 1 | 295 | 51,284 | $0.0211 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 253 | 51,629 | $0.0207 | 6.6s |
| 14 | claude-sonnet-4-6 | 1 | 221 | 52,015 | $0.0202 | 3.4s |
| 15 | claude-sonnet-4-6 | 1 | 11,142 | 52,358 | $0.1840 | 114.0s |
| 16 | claude-sonnet-4-6 | 1 | 6,234 | 52,671 | $0.1514 | 63.0s |
| 17 | claude-sonnet-4-6 | 1 | 261 | 63,905 | $0.0476 | 7.5s |
| 18 | claude-sonnet-4-6 | 1 | 172 | 70,440 | $0.0249 | 6.2s |
| 19 | claude-sonnet-4-6 | 1 | 203 | 70,743 | $0.0251 | 7.4s |
| 20 | claude-sonnet-4-6 | 1 | 318 | 70,953 | $0.0276 | 7.4s |
| 21 | claude-sonnet-4-6 | 1 | 407 | 71,904 | $0.0291 | 9.9s |
| 22 | claude-sonnet-4-6 | 1 | 210 | 74,242 | $0.0276 | 8.0s |
| 23 | claude-sonnet-4-6 | 1 | 186 | 74,834 | $0.0274 | 6.0s |
| 24 | claude-sonnet-4-6 | 1 | 174 | 75,413 | $0.0261 | 5.1s |
| 25 | claude-sonnet-4-6 | 1 | 174 | 75,634 | $0.0260 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 268 | 75,826 | $0.0275 | 4.2s |
| 27 | claude-sonnet-4-6 | 1 | 132 | 76,018 | $0.0260 | 3.1s |
| 28 | claude-sonnet-4-6 | 1 | 120 | 76,493 | $0.0255 | 2.9s |
| 29 | claude-sonnet-4-6 | 1 | 125 | 76,823 | $0.0256 | 3.8s |
| 30 | claude-sonnet-4-6 | 1 | 274 | 77,016 | $0.0277 | 4.1s |
| 31 | claude-sonnet-4-6 | 1 | 794 | 77,154 | $0.0362 | 17.7s |
| 32 | claude-sonnet-4-6 | 1 | 259 | 77,470 | $0.0304 | 3.5s |
| 33 | claude-sonnet-4-6 | 1 | 181 | 77,470 | $0.0304 | 4.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
