# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 2
**Date:** 2026-04-06
**Started:** 2026-04-06T16:22:10-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 27 |
| Total output tokens     | 10,200 |
| Total tokens            | 10,227 |
| Cache read tokens       | 1,411,072 |
| Cache creation tokens   | 21,168 |
| Total cost (USD)        | $0.6558 |
| Total API time          | 160.2s |
| API calls               | 27 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 731 | 37,435 | $0.0503 | 10.8s |
| 2 | claude-sonnet-4-6 | 1 | 230 | 44,939 | $0.0198 | 3.0s |
| 3 | claude-sonnet-4-6 | 1 | 230 | 45,976 | $0.0198 | 4.4s |
| 4 | claude-sonnet-4-6 | 1 | 211 | 46,653 | $0.0182 | 3.4s |
| 5 | claude-sonnet-4-6 | 1 | 1,811 | 46,925 | $0.0425 | 17.9s |
| 6 | claude-sonnet-4-6 | 1 | 230 | 47,247 | $0.0248 | 3.9s |
| 7 | claude-sonnet-4-6 | 1 | 173 | 49,150 | $0.0184 | 3.9s |
| 8 | claude-sonnet-4-6 | 1 | 190 | 49,422 | $0.0192 | 4.2s |
| 9 | claude-sonnet-4-6 | 1 | 175 | 49,820 | $0.0191 | 2.9s |
| 10 | claude-sonnet-4-6 | 1 | 224 | 50,235 | $0.0199 | 4.1s |
| 11 | claude-sonnet-4-6 | 1 | 276 | 50,635 | $0.0210 | 8.3s |
| 12 | claude-sonnet-4-6 | 1 | 230 | 51,084 | $0.0199 | 4.4s |
| 13 | claude-sonnet-4-6 | 1 | 352 | 51,374 | $0.0217 | 4.4s |
| 14 | claude-sonnet-4-6 | 1 | 364 | 51,646 | $0.0226 | 4.6s |
| 15 | claude-sonnet-4-6 | 1 | 325 | 52,546 | $0.0226 | 6.5s |
| 16 | claude-sonnet-4-6 | 1 | 710 | 53,066 | $0.0281 | 7.8s |
| 17 | claude-sonnet-4-6 | 1 | 544 | 53,483 | $0.0272 | 10.5s |
| 18 | claude-sonnet-4-6 | 1 | 910 | 54,285 | $0.0330 | 12.0s |
| 19 | claude-sonnet-4-6 | 1 | 764 | 55,113 | $0.0318 | 8.4s |
| 20 | claude-sonnet-4-6 | 1 | 172 | 56,115 | $0.0226 | 4.4s |
| 21 | claude-sonnet-4-6 | 1 | 230 | 58,084 | $0.0227 | 4.1s |
| 22 | claude-sonnet-4-6 | 1 | 172 | 58,574 | $0.0212 | 6.6s |
| 23 | claude-sonnet-4-6 | 1 | 183 | 58,846 | $0.0211 | 6.6s |
| 24 | claude-sonnet-4-6 | 1 | 141 | 59,036 | $0.0216 | 4.0s |
| 25 | claude-sonnet-4-6 | 1 | 201 | 59,509 | $0.0220 | 3.2s |
| 26 | claude-sonnet-4-6 | 1 | 187 | 59,803 | $0.0218 | 2.9s |
| 27 | claude-sonnet-4-6 | 1 | 234 | 60,071 | $0.0229 | 3.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
