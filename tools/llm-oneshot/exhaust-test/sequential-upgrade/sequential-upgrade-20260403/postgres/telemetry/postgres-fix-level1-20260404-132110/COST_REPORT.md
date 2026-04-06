# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-04T13:21:10-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 23 |
| Total output tokens     | 5,569 |
| Total tokens            | 5,592 |
| Cache read tokens       | 882,465 |
| Cache creation tokens   | 25,365 |
| Total cost (USD)        | $0.4435 |
| Total API time          | 123.0s |
| API calls               | 21 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 157 | 20,668 | $0.0431 | 3.8s |
| 2 | claude-sonnet-4-6 | 1 | 162 | 32,026 | $0.0151 | 4.1s |
| 3 | claude-sonnet-4-6 | 1 | 204 | 33,955 | $0.0142 | 5.1s |
| 4 | claude-sonnet-4-6 | 1 | 141 | 36,860 | $0.0142 | 3.5s |
| 5 | claude-sonnet-4-6 | 1 | 127 | 36,860 | $0.0150 | 4.9s |
| 6 | claude-sonnet-4-6 | 1 | 298 | 37,138 | $0.0231 | 8.8s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 39,123 | $0.0210 | 4.3s |
| 8 | claude-sonnet-4-6 | 1 | 161 | 39,123 | $0.0252 | 3.1s |
| 9 | claude-sonnet-4-6 | 1 | 843 | 42,056 | $0.0278 | 11.4s |
| 10 | claude-sonnet-4-6 | 1 | 161 | 43,687 | $0.0173 | 6.5s |
| 11 | claude-sonnet-4-6 | 1 | 341 | 43,687 | $0.0218 | 8.3s |
| 12 | claude-sonnet-4-6 | 1 | 907 | 44,648 | $0.0291 | 10.1s |
| 13 | claude-sonnet-4-6 | 1 | 244 | 45,205 | $0.0210 | 7.3s |
| 14 | claude-sonnet-4-6 | 1 | 162 | 46,205 | $0.0188 | 4.5s |
| 15 | claude-sonnet-4-6 | 1 | 208 | 47,608 | $0.0188 | 6.0s |
| 16 | claude-sonnet-4-6 | 1 | 368 | 47,973 | $0.0208 | 9.3s |
| 17 | claude-sonnet-4-6 | 1 | 174 | 48,196 | $0.0188 | 4.4s |
| 18 | claude-sonnet-4-6 | 1 | 162 | 48,657 | $0.0190 | 3.4s |
| 19 | claude-sonnet-4-6 | 1 | 326 | 48,657 | $0.0232 | 5.0s |
| 20 | claude-sonnet-4-6 | 1 | 167 | 49,653 | $0.0190 | 6.0s |
| 21 | claude-sonnet-4-6 | 1 | 95 | 50,480 | $0.0173 | 3.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
