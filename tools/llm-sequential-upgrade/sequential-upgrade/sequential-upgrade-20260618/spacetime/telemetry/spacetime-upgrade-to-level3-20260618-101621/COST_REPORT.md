# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 3
**Date:** 2026-06-18
**Started:** 2026-06-18T10:16:22-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,413 |
| Total output tokens     | 17,784 |
| Total tokens            | 19,197 |
| Cache read tokens       | 1,084,181 |
| Cache creation tokens   | 55,197 |
| Total cost (USD)        | $0.8003 |
| Total API time          | 254.6s |
| API calls               | 18 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,394 | 19 | 0 | $0.0015 | 1.2s |
| 2 | claude-sonnet-4-6 | 3 | 317 | 20,621 | $0.0673 | 5.4s |
| 3 | claude-sonnet-4-6 | 1 | 260 | 35,657 | $0.0316 | 4.7s |
| 4 | claude-sonnet-4-6 | 1 | 9,291 | 40,199 | $0.2100 | 133.2s |
| 5 | claude-sonnet-4-6 | 1 | 2,603 | 55,808 | $0.0910 | 27.2s |
| 6 | claude-sonnet-4-6 | 1 | 210 | 65,204 | $0.0329 | 4.0s |
| 7 | claude-sonnet-4-6 | 1 | 196 | 67,912 | $0.0250 | 4.2s |
| 8 | claude-sonnet-4-6 | 1 | 293 | 68,367 | $0.0262 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 1,552 | 68,700 | $0.0496 | 24.1s |
| 10 | claude-sonnet-4-6 | 1 | 533 | 70,233 | $0.0353 | 7.0s |
| 11 | claude-sonnet-4-6 | 1 | 290 | 71,904 | $0.0284 | 4.2s |
| 12 | claude-sonnet-4-6 | 1 | 343 | 72,556 | $0.0287 | 4.7s |
| 13 | claude-sonnet-4-6 | 1 | 814 | 73,045 | $0.0358 | 10.8s |
| 14 | claude-sonnet-4-6 | 1 | 505 | 73,488 | $0.0331 | 7.0s |
| 15 | claude-sonnet-4-6 | 1 | 175 | 74,402 | $0.0272 | 4.1s |
| 16 | claude-sonnet-4-6 | 1 | 158 | 75,007 | $0.0256 | 3.0s |
| 17 | claude-sonnet-4-6 | 1 | 103 | 75,200 | $0.0258 | 2.9s |
| 18 | claude-sonnet-4-6 | 1 | 122 | 75,878 | $0.0252 | 2.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
