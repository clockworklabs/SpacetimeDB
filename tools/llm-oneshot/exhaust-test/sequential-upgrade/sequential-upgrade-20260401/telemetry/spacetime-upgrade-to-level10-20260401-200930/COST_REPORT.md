# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-04-02
**Started:** 2026-04-01T20:09:30-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 26 |
| Total output tokens     | 10,104 |
| Total tokens            | 10,130 |
| Cache read tokens       | 1,660,518 |
| Cache creation tokens   | 44,076 |
| Total cost (USD)        | $0.8151 |
| Total API time          | 181.7s |
| API calls               | 24 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 267 | 20,619 | $0.0663 | 6.3s |
| 2 | claude-sonnet-4-6 | 1 | 162 | 35,581 | $0.0281 | 4.2s |
| 3 | claude-sonnet-4-6 | 1 | 131 | 52,558 | $0.0185 | 4.4s |
| 4 | claude-sonnet-4-6 | 1 | 148 | 52,773 | $0.0298 | 4.8s |
| 5 | claude-sonnet-4-6 | 1 | 148 | 55,893 | $0.0311 | 5.7s |
| 6 | claude-sonnet-4-6 | 1 | 148 | 59,123 | $0.0318 | 7.2s |
| 7 | claude-sonnet-4-6 | 1 | 149 | 65,276 | $0.0330 | 4.3s |
| 8 | claude-sonnet-4-6 | 1 | 3,459 | 68,251 | $0.0816 | 57.6s |
| 9 | claude-sonnet-4-6 | 1 | 765 | 70,701 | $0.0458 | 13.4s |
| 10 | claude-sonnet-4-6 | 1 | 270 | 74,200 | $0.0296 | 3.7s |
| 11 | claude-sonnet-4-6 | 1 | 1,030 | 75,069 | $0.0391 | 13.5s |
| 12 | claude-sonnet-4-6 | 1 | 270 | 75,381 | $0.0309 | 3.3s |
| 13 | claude-sonnet-4-6 | 1 | 157 | 76,515 | $0.0265 | 3.3s |
| 14 | claude-sonnet-4-6 | 1 | 296 | 77,692 | $0.0288 | 4.8s |
| 15 | claude-sonnet-4-6 | 1 | 204 | 77,974 | $0.0277 | 3.7s |
| 16 | claude-sonnet-4-6 | 1 | 224 | 78,312 | $0.0280 | 3.8s |
| 17 | claude-sonnet-4-6 | 1 | 853 | 78,615 | $0.0375 | 10.7s |
| 18 | claude-sonnet-4-6 | 1 | 270 | 78,919 | $0.0312 | 3.7s |
| 19 | claude-sonnet-4-6 | 1 | 270 | 80,515 | $0.0294 | 4.7s |
| 20 | claude-sonnet-4-6 | 1 | 140 | 80,828 | $0.0275 | 3.0s |
| 21 | claude-sonnet-4-6 | 1 | 160 | 81,140 | $0.0274 | 3.7s |
| 22 | claude-sonnet-4-6 | 1 | 158 | 81,315 | $0.0272 | 3.6s |
| 23 | claude-sonnet-4-6 | 1 | 157 | 81,434 | $0.0283 | 3.9s |
| 24 | claude-sonnet-4-6 | 1 | 268 | 81,834 | $0.0300 | 4.3s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
