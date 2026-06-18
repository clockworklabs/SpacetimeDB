# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-06-18
**Started:** 2026-06-18T15:29:18-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 882 |
| Total output tokens     | 8,424 |
| Total tokens            | 9,306 |
| Cache read tokens       | 566,729 |
| Cache creation tokens   | 45,800 |
| Total cost (USD)        | $0.4689 |
| Total API time          | 139.6s |
| API calls               | 11 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 870 | 14 | 0 | $0.0009 | 1.0s |
| 2 | claude-sonnet-4-6 | 3 | 175 | 20,621 | $0.0646 | 3.5s |
| 3 | claude-sonnet-4-6 | 1 | 5,943 | 35,945 | $0.1818 | 91.5s |
| 4 | claude-sonnet-4-6 | 1 | 164 | 57,776 | $0.0425 | 3.4s |
| 5 | claude-sonnet-4-6 | 1 | 172 | 63,838 | $0.0224 | 3.4s |
| 6 | claude-sonnet-4-6 | 1 | 158 | 64,020 | $0.0223 | 5.1s |
| 7 | claude-sonnet-4-6 | 1 | 109 | 64,212 | $0.0219 | 2.5s |
| 8 | claude-sonnet-4-6 | 1 | 134 | 64,487 | $0.0218 | 2.5s |
| 9 | claude-sonnet-4-6 | 1 | 445 | 64,609 | $0.0288 | 9.1s |
| 10 | claude-sonnet-4-6 | 1 | 886 | 65,328 | $0.0350 | 8.7s |
| 11 | claude-sonnet-4-6 | 1 | 224 | 65,893 | $0.0268 | 8.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
