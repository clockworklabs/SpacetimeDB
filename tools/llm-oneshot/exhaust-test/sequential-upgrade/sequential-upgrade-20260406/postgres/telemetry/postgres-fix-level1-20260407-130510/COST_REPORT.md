# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 1
**Date:** 2026-04-07
**Started:** 2026-04-07T13:05:10-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 18 |
| Total output tokens     | 5,807 |
| Total tokens            | 5,825 |
| Cache read tokens       | 682,516 |
| Cache creation tokens   | 24,173 |
| Total cost (USD)        | $0.3826 |
| Total API time          | 123.3s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 166 | 20,510 | $0.0408 | 4.0s |
| 2 | claude-sonnet-4-6 | 1 | 142 | 29,622 | $0.0117 | 2.9s |
| 3 | claude-sonnet-4-6 | 1 | 207 | 31,341 | $0.0203 | 8.0s |
| 4 | claude-sonnet-4-6 | 1 | 941 | 33,414 | $0.0296 | 19.9s |
| 5 | claude-sonnet-4-6 | 1 | 602 | 38,479 | $0.0225 | 12.3s |
| 6 | claude-sonnet-4-6 | 1 | 450 | 40,069 | $0.0402 | 12.3s |
| 7 | claude-sonnet-4-6 | 1 | 881 | 45,787 | $0.0303 | 9.3s |
| 8 | claude-sonnet-4-6 | 1 | 184 | 46,686 | $0.0205 | 5.6s |
| 9 | claude-sonnet-4-6 | 1 | 661 | 47,677 | $0.0271 | 6.9s |
| 10 | claude-sonnet-4-6 | 1 | 168 | 48,457 | $0.0200 | 3.8s |
| 11 | claude-sonnet-4-6 | 1 | 120 | 49,228 | $0.0177 | 5.9s |
| 12 | claude-sonnet-4-6 | 1 | 102 | 49,670 | $0.0172 | 3.2s |
| 13 | claude-sonnet-4-6 | 1 | 95 | 50,057 | $0.0172 | 2.7s |
| 14 | claude-sonnet-4-6 | 1 | 173 | 50,259 | $0.0182 | 4.9s |
| 15 | claude-sonnet-4-6 | 1 | 726 | 50,402 | $0.0277 | 16.0s |
| 16 | claude-sonnet-4-6 | 1 | 189 | 50,858 | $0.0216 | 5.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
