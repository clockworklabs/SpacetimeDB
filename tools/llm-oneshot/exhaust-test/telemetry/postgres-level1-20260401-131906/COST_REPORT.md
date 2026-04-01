# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-01
**Started:** 2026-04-01T13:19:06-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 27 |
| Total output tokens     | 38,854 |
| Total tokens            | 38,881 |
| Cache read tokens       | 1,361,244 |
| Cache creation tokens   | 34,338 |
| Total cost (USD)        | $1.1200 |
| Total API time          | 480.0s |
| API calls               | 25 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 827 | 20,619 | $0.0587 | 16.5s |
| 2 | claude-sonnet-4-6 | 1 | 187 | 32,220 | $0.0139 | 5.4s |
| 3 | claude-sonnet-4-6 | 1 | 8,000 | 32,220 | $0.1349 | 95.2s |
| 4 | claude-sonnet-4-6 | 1 | 9,876 | 33,614 | $0.1582 | 116.7s |
| 5 | claude-sonnet-4-6 | 1 | 916 | 43,498 | $0.0277 | 11.4s |
| 6 | claude-sonnet-4-6 | 1 | 763 | 43,750 | $0.0294 | 8.2s |
| 7 | claude-sonnet-4-6 | 1 | 4,777 | 45,030 | $0.0883 | 48.4s |
| 8 | claude-sonnet-4-6 | 1 | 253 | 45,871 | $0.0358 | 5.2s |
| 9 | claude-sonnet-4-6 | 1 | 1,223 | 50,726 | $0.0347 | 12.0s |
| 10 | claude-sonnet-4-6 | 1 | 3,112 | 51,021 | $0.0683 | 32.3s |
| 11 | claude-sonnet-4-6 | 1 | 5,288 | 52,698 | $0.1071 | 57.6s |
| 12 | claude-sonnet-4-6 | 1 | 253 | 55,888 | $0.0407 | 5.9s |
| 13 | claude-sonnet-4-6 | 1 | 153 | 61,254 | $0.0218 | 3.6s |
| 14 | claude-sonnet-4-6 | 1 | 161 | 61,549 | $0.0220 | 3.9s |
| 15 | claude-sonnet-4-6 | 1 | 940 | 61,845 | $0.0362 | 17.1s |
| 16 | claude-sonnet-4-6 | 1 | 225 | 65,285 | $0.0239 | 4.1s |
| 17 | claude-sonnet-4-6 | 1 | 332 | 65,538 | $0.0256 | 4.3s |
| 18 | claude-sonnet-4-6 | 1 | 169 | 66,132 | $0.0240 | 4.4s |
| 19 | claude-sonnet-4-6 | 1 | 253 | 66,559 | $0.0246 | 4.3s |
| 20 | claude-sonnet-4-6 | 1 | 303 | 66,781 | $0.0257 | 4.2s |
| 21 | claude-sonnet-4-6 | 1 | 263 | 67,076 | $0.0255 | 3.7s |
| 22 | claude-sonnet-4-6 | 1 | 123 | 67,452 | $0.0232 | 4.0s |
| 23 | claude-sonnet-4-6 | 1 | 99 | 67,912 | $0.0225 | 3.2s |
| 24 | claude-sonnet-4-6 | 1 | 107 | 68,204 | $0.0227 | 3.3s |
| 25 | claude-sonnet-4-6 | 1 | 251 | 68,502 | $0.0247 | 5.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
