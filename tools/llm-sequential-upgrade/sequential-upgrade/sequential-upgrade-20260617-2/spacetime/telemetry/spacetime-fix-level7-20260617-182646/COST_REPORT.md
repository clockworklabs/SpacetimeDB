# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-06-17
**Started:** 2026-06-17T18:26:46-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 889 |
| Total output tokens     | 6,843 |
| Total tokens            | 7,732 |
| Cache read tokens       | 665,495 |
| Cache creation tokens   | 30,291 |
| Total cost (USD)        | $0.4848 |
| Total API time          | 98.7s |
| API calls               | 16 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 872 | 14 | 0 | $0.0009 | 1.3s |
| 2 | claude-sonnet-4-6 | 3 | 181 | 20,621 | $0.0959 | 3.2s |
| 3 | claude-sonnet-4-6 | 1 | 233 | 35,120 | $0.0182 | 3.1s |
| 4 | claude-sonnet-4-6 | 1 | 2,370 | 35,822 | $0.0882 | 34.2s |
| 5 | claude-sonnet-4-6 | 1 | 826 | 42,808 | $0.0402 | 8.5s |
| 6 | claude-sonnet-4-6 | 1 | 93 | 45,304 | $0.0207 | 2.4s |
| 7 | claude-sonnet-4-6 | 1 | 195 | 46,256 | $0.0176 | 3.2s |
| 8 | claude-sonnet-4-6 | 1 | 1,032 | 46,392 | $0.0320 | 10.2s |
| 9 | claude-sonnet-4-6 | 1 | 195 | 46,822 | $0.0239 | 2.7s |
| 10 | claude-sonnet-4-6 | 1 | 288 | 47,980 | $0.0225 | 4.6s |
| 11 | claude-sonnet-4-6 | 1 | 263 | 48,615 | $0.0223 | 4.2s |
| 12 | claude-sonnet-4-6 | 1 | 122 | 49,243 | $0.0200 | 2.9s |
| 13 | claude-sonnet-4-6 | 1 | 181 | 49,806 | $0.0185 | 2.9s |
| 14 | claude-sonnet-4-6 | 1 | 246 | 49,951 | $0.0204 | 3.8s |
| 15 | claude-sonnet-4-6 | 1 | 134 | 50,245 | $0.0187 | 2.9s |
| 16 | claude-sonnet-4-6 | 1 | 470 | 50,510 | $0.0246 | 8.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
