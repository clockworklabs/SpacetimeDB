# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-03
**Started:** 2026-04-03T19:26:05-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 13 |
| Total output tokens     | 3,657 |
| Total tokens            | 3,670 |
| Cache read tokens       | 497,683 |
| Cache creation tokens   | 5,410 |
| Total cost (USD)        | $0.2245 |
| Total API time          | 68.6s |
| API calls               | 13 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 190 | 34,824 | $0.0145 | 3.8s |
| 2 | claude-sonnet-4-6 | 1 | 398 | 35,137 | $0.0185 | 6.3s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 35,660 | $0.0148 | 3.1s |
| 4 | claude-sonnet-4-6 | 1 | 482 | 36,112 | $0.0206 | 7.3s |
| 5 | claude-sonnet-4-6 | 1 | 223 | 36,782 | $0.0170 | 4.7s |
| 6 | claude-sonnet-4-6 | 1 | 161 | 37,718 | $0.0150 | 2.3s |
| 7 | claude-sonnet-4-6 | 1 | 396 | 38,057 | $0.0192 | 6.0s |
| 8 | claude-sonnet-4-6 | 1 | 306 | 38,541 | $0.0181 | 4.1s |
| 9 | claude-sonnet-4-6 | 1 | 93 | 40,064 | $0.0142 | 5.4s |
| 10 | claude-sonnet-4-6 | 1 | 96 | 40,387 | $0.0142 | 4.0s |
| 11 | claude-sonnet-4-6 | 1 | 182 | 40,766 | $0.0166 | 3.9s |
| 12 | claude-sonnet-4-6 | 1 | 175 | 41,755 | $0.0156 | 3.2s |
| 13 | claude-sonnet-4-6 | 1 | 794 | 41,880 | $0.0263 | 14.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
