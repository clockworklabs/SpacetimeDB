# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-04T14:55:23-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 12 |
| Total output tokens     | 3,633 |
| Total tokens            | 3,645 |
| Cache read tokens       | 423,941 |
| Cache creation tokens   | 22,423 |
| Total cost (USD)        | $0.2658 |
| Total API time          | 60.7s |
| API calls               | 10 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 160 | 20,668 | $0.0519 | 3.0s |
| 2 | claude-sonnet-4-6 | 1 | 162 | 32,657 | $0.0129 | 2.5s |
| 3 | claude-sonnet-4-6 | 1 | 510 | 32,846 | $0.0304 | 7.5s |
| 4 | claude-sonnet-4-6 | 1 | 167 | 40,872 | $0.0162 | 3.0s |
| 5 | claude-sonnet-4-6 | 1 | 944 | 41,258 | $0.0292 | 15.6s |
| 6 | claude-sonnet-4-6 | 1 | 484 | 44,780 | $0.0361 | 5.0s |
| 7 | claude-sonnet-4-6 | 1 | 223 | 51,994 | $0.0194 | 3.6s |
| 8 | claude-sonnet-4-6 | 1 | 174 | 52,103 | $0.0202 | 3.7s |
| 9 | claude-sonnet-4-6 | 1 | 161 | 53,271 | $0.0192 | 3.8s |
| 10 | claude-sonnet-4-6 | 1 | 648 | 53,492 | $0.0303 | 13.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
