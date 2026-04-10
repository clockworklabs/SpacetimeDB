# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 10
**Date:** 2026-04-07
**Started:** 2026-04-07T12:45:48-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 30 |
| Total output tokens     | 8,490 |
| Total tokens            | 8,520 |
| Cache read tokens       | 1,797,240 |
| Cache creation tokens   | 41,196 |
| Total cost (USD)        | $0.8211 |
| Total API time          | 136.6s |
| API calls               | 28 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 212 | 20,510 | $0.0555 | 5.2s |
| 2 | claude-sonnet-4-6 | 1 | 143 | 50,753 | $0.0187 | 2.2s |
| 3 | claude-sonnet-4-6 | 1 | 160 | 50,753 | $0.0261 | 2.7s |
| 4 | claude-sonnet-4-6 | 1 | 212 | 51,100 | $0.0339 | 4.8s |
| 5 | claude-sonnet-4-6 | 1 | 160 | 55,205 | $0.0268 | 2.6s |
| 6 | claude-sonnet-4-6 | 1 | 160 | 55,205 | $0.0347 | 4.2s |
| 7 | claude-sonnet-4-6 | 1 | 562 | 59,402 | $0.0386 | 12.5s |
| 8 | claude-sonnet-4-6 | 1 | 160 | 62,703 | $0.0279 | 2.5s |
| 9 | claude-sonnet-4-6 | 1 | 1,661 | 64,309 | $0.0465 | 24.9s |
| 10 | claude-sonnet-4-6 | 1 | 171 | 64,931 | $0.0284 | 2.3s |
| 11 | claude-sonnet-4-6 | 1 | 613 | 64,931 | $0.0359 | 7.9s |
| 12 | claude-sonnet-4-6 | 1 | 171 | 66,848 | $0.0253 | 3.1s |
| 13 | claude-sonnet-4-6 | 1 | 587 | 67,572 | $0.0299 | 7.0s |
| 14 | claude-sonnet-4-6 | 1 | 610 | 67,785 | $0.0321 | 5.3s |
| 15 | claude-sonnet-4-6 | 1 | 575 | 68,483 | $0.0318 | 5.5s |
| 16 | claude-sonnet-4-6 | 1 | 171 | 69,185 | $0.0258 | 3.0s |
| 17 | claude-sonnet-4-6 | 1 | 362 | 69,852 | $0.0272 | 4.9s |
| 18 | claude-sonnet-4-6 | 1 | 171 | 70,065 | $0.0254 | 4.7s |
| 19 | claude-sonnet-4-6 | 1 | 173 | 70,538 | $0.0246 | 2.6s |
| 20 | claude-sonnet-4-6 | 1 | 166 | 70,751 | $0.0244 | 3.9s |
| 21 | claude-sonnet-4-6 | 1 | 103 | 70,942 | $0.0246 | 2.2s |
| 22 | claude-sonnet-4-6 | 1 | 213 | 71,410 | $0.0251 | 2.9s |
| 23 | claude-sonnet-4-6 | 1 | 197 | 71,533 | $0.0253 | 3.0s |
| 24 | claude-sonnet-4-6 | 1 | 213 | 71,772 | $0.0258 | 3.2s |
| 25 | claude-sonnet-4-6 | 1 | 168 | 72,054 | $0.0262 | 4.0s |
| 26 | claude-sonnet-4-6 | 1 | 208 | 72,597 | $0.0259 | 3.8s |
| 27 | claude-sonnet-4-6 | 1 | 180 | 72,850 | $0.0259 | 3.5s |
| 28 | claude-sonnet-4-6 | 1 | 8 | 73,201 | $0.0229 | 2.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
