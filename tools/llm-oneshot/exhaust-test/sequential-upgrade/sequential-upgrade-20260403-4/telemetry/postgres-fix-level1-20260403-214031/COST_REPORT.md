# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-03T21:40:31-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 31 |
| Total output tokens     | 10,525 |
| Total tokens            | 10,556 |
| Cache read tokens       | 1,729,974 |
| Cache creation tokens   | 37,270 |
| Total cost (USD)        | $0.8167 |
| Total API time          | 224.4s |
| API calls               | 31 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 391 | 29,991 | $0.0391 | 8.8s |
| 2 | claude-sonnet-4-6 | 1 | 161 | 36,451 | $0.0211 | 3.3s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 38,516 | $0.0236 | 4.7s |
| 4 | claude-sonnet-4-6 | 1 | 535 | 41,071 | $0.0307 | 11.7s |
| 5 | claude-sonnet-4-6 | 1 | 161 | 43,840 | $0.0225 | 3.8s |
| 6 | claude-sonnet-4-6 | 1 | 161 | 45,689 | $0.0220 | 3.1s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 49,563 | $0.0214 | 15.3s |
| 8 | claude-sonnet-4-6 | 1 | 162 | 50,672 | $0.0199 | 4.2s |
| 9 | claude-sonnet-4-6 | 1 | 188 | 51,282 | $0.0229 | 4.1s |
| 10 | claude-sonnet-4-6 | 1 | 161 | 52,521 | $0.0243 | 5.5s |
| 11 | claude-sonnet-4-6 | 1 | 161 | 52,521 | $0.0292 | 2.6s |
| 12 | claude-sonnet-4-6 | 1 | 685 | 54,145 | $0.0354 | 12.9s |
| 13 | claude-sonnet-4-6 | 1 | 444 | 56,516 | $0.0263 | 6.5s |
| 14 | claude-sonnet-4-6 | 1 | 2,076 | 57,244 | $0.0504 | 21.5s |
| 15 | claude-sonnet-4-6 | 1 | 194 | 57,800 | $0.0285 | 5.4s |
| 16 | claude-sonnet-4-6 | 1 | 341 | 59,988 | $0.0240 | 6.4s |
| 17 | claude-sonnet-4-6 | 1 | 295 | 60,224 | $0.0241 | 6.5s |
| 18 | claude-sonnet-4-6 | 1 | 400 | 60,658 | $0.0257 | 5.5s |
| 19 | claude-sonnet-4-6 | 1 | 540 | 61,046 | $0.0283 | 7.5s |
| 20 | claude-sonnet-4-6 | 1 | 185 | 61,539 | $0.0236 | 3.6s |
| 21 | claude-sonnet-4-6 | 1 | 656 | 62,172 | $0.0318 | 8.5s |
| 22 | claude-sonnet-4-6 | 1 | 194 | 63,054 | $0.0246 | 3.7s |
| 23 | claude-sonnet-4-6 | 1 | 166 | 63,054 | $0.0251 | 4.0s |
| 24 | claude-sonnet-4-6 | 1 | 157 | 64,039 | $0.0223 | 2.7s |
| 25 | claude-sonnet-4-6 | 1 | 128 | 64,223 | $0.0218 | 10.2s |
| 26 | claude-sonnet-4-6 | 1 | 97 | 64,539 | $0.0215 | 4.5s |
| 27 | claude-sonnet-4-6 | 1 | 171 | 64,732 | $0.0227 | 10.2s |
| 28 | claude-sonnet-4-6 | 1 | 91 | 64,926 | $0.0223 | 6.3s |
| 29 | claude-sonnet-4-6 | 1 | 192 | 65,810 | $0.0232 | 3.6s |
| 30 | claude-sonnet-4-6 | 1 | 172 | 65,957 | $0.0232 | 10.9s |
| 31 | claude-sonnet-4-6 | 1 | 938 | 66,191 | $0.0352 | 17.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
