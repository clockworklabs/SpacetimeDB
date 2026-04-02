# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 7
**Date:** 2026-04-02
**Started:** 2026-04-02T15:57:07-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 24 |
| Total output tokens     | 30,849 |
| Total tokens            | 30,873 |
| Cache read tokens       | 1,206,664 |
| Cache creation tokens   | 40,225 |
| Total cost (USD)        | $0.9756 |
| Total API time          | 403.2s |
| API calls               | 22 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 195 | 20,668 | $0.0521 | 9.6s |
| 2 | claude-sonnet-4-6 | 1 | 6,316 | 32,133 | $0.1110 | 98.7s |
| 3 | claude-sonnet-4-6 | 1 | 214 | 41,180 | $0.0160 | 5.6s |
| 4 | claude-sonnet-4-6 | 1 | 620 | 41,552 | $0.0235 | 7.0s |
| 5 | claude-sonnet-4-6 | 1 | 772 | 42,023 | $0.0277 | 13.8s |
| 6 | claude-sonnet-4-6 | 1 | 5,773 | 42,962 | $0.1027 | 59.3s |
| 7 | claude-sonnet-4-6 | 1 | 1,137 | 43,826 | $0.0522 | 11.6s |
| 8 | claude-sonnet-4-6 | 1 | 219 | 49,691 | $0.0248 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 7,429 | 51,444 | $0.1280 | 77.0s |
| 10 | claude-sonnet-4-6 | 1 | 5,503 | 51,755 | $0.1263 | 54.4s |
| 11 | claude-sonnet-4-6 | 1 | 207 | 59,276 | $0.0419 | 4.8s |
| 12 | claude-sonnet-4-6 | 1 | 167 | 64,871 | $0.0229 | 6.4s |
| 13 | claude-sonnet-4-6 | 1 | 174 | 65,120 | $0.0233 | 3.5s |
| 14 | claude-sonnet-4-6 | 1 | 167 | 65,430 | $0.0234 | 6.1s |
| 15 | claude-sonnet-4-6 | 1 | 207 | 65,764 | $0.0237 | 3.4s |
| 16 | claude-sonnet-4-6 | 1 | 174 | 65,984 | $0.0233 | 3.0s |
| 17 | claude-sonnet-4-6 | 1 | 185 | 66,233 | $0.0234 | 5.5s |
| 18 | claude-sonnet-4-6 | 1 | 216 | 66,425 | $0.0250 | 5.2s |
| 19 | claude-sonnet-4-6 | 1 | 130 | 66,901 | $0.0230 | 3.7s |
| 20 | claude-sonnet-4-6 | 1 | 165 | 67,322 | $0.0234 | 3.3s |
| 21 | claude-sonnet-4-6 | 1 | 674 | 67,897 | $0.0316 | 14.5s |
| 22 | claude-sonnet-4-6 | 1 | 205 | 68,207 | $0.0264 | 3.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
