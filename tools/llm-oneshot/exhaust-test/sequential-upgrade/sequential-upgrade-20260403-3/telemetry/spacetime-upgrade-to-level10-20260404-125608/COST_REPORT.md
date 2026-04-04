# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-04-04
**Started:** 2026-04-04T12:56:08-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 16 |
| Total output tokens     | 6,249 |
| Total tokens            | 6,265 |
| Cache read tokens       | 1,297,573 |
| Cache creation tokens   | 22,625 |
| Total cost (USD)        | $0.5679 |
| Total API time          | 99.4s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 145 | 50,820 | $0.0316 | 2.2s |
| 2 | claude-sonnet-4-6 | 1 | 162 | 54,604 | $0.0304 | 4.2s |
| 3 | claude-sonnet-4-6 | 1 | 162 | 57,701 | $0.0322 | 4.4s |
| 4 | claude-sonnet-4-6 | 1 | 720 | 78,803 | $0.0471 | 16.4s |
| 5 | claude-sonnet-4-6 | 1 | 590 | 82,177 | $0.0423 | 9.0s |
| 6 | claude-sonnet-4-6 | 1 | 402 | 84,512 | $0.0358 | 5.6s |
| 7 | claude-sonnet-4-6 | 1 | 270 | 85,693 | $0.0321 | 4.6s |
| 8 | claude-sonnet-4-6 | 1 | 535 | 86,312 | $0.0354 | 8.0s |
| 9 | claude-sonnet-4-6 | 1 | 594 | 86,695 | $0.0373 | 9.9s |
| 10 | claude-sonnet-4-6 | 1 | 406 | 87,324 | $0.0349 | 5.4s |
| 11 | claude-sonnet-4-6 | 1 | 600 | 88,512 | $0.0385 | 5.5s |
| 12 | claude-sonnet-4-6 | 1 | 634 | 89,301 | $0.0393 | 5.8s |
| 13 | claude-sonnet-4-6 | 1 | 162 | 90,827 | $0.0304 | 3.3s |
| 14 | claude-sonnet-4-6 | 1 | 516 | 90,827 | $0.0372 | 5.4s |
| 15 | claude-sonnet-4-6 | 1 | 183 | 91,418 | $0.0325 | 7.2s |
| 16 | claude-sonnet-4-6 | 1 | 168 | 92,047 | $0.0309 | 2.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
