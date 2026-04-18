# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 4
**Date:** 2026-04-03
**Started:** 2026-04-03T15:03:33-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 25 |
| Total output tokens     | 7,911 |
| Total tokens            | 7,936 |
| Cache read tokens       | 1,219,519 |
| Cache creation tokens   | 31,677 |
| Total cost (USD)        | $0.6034 |
| Total API time          | 119.2s |
| API calls               | 23 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 274 | 20,668 | $0.0510 | 4.1s |
| 2 | claude-sonnet-4-6 | 1 | 790 | 40,208 | $0.0582 | 15.1s |
| 3 | claude-sonnet-4-6 | 1 | 200 | 49,347 | $0.0235 | 3.7s |
| 4 | claude-sonnet-4-6 | 1 | 223 | 50,857 | $0.0195 | 3.4s |
| 5 | claude-sonnet-4-6 | 1 | 331 | 51,093 | $0.0216 | 3.8s |
| 6 | claude-sonnet-4-6 | 1 | 194 | 51,428 | $0.0200 | 3.1s |
| 7 | claude-sonnet-4-6 | 1 | 1,082 | 51,871 | $0.0327 | 12.2s |
| 8 | claude-sonnet-4-6 | 1 | 194 | 52,107 | $0.0230 | 3.3s |
| 9 | claude-sonnet-4-6 | 1 | 288 | 52,107 | $0.0253 | 5.3s |
| 10 | claude-sonnet-4-6 | 1 | 304 | 53,537 | $0.0221 | 4.6s |
| 11 | claude-sonnet-4-6 | 1 | 342 | 53,918 | $0.0228 | 5.3s |
| 12 | claude-sonnet-4-6 | 1 | 257 | 54,315 | $0.0218 | 3.9s |
| 13 | claude-sonnet-4-6 | 1 | 542 | 54,750 | $0.0259 | 6.9s |
| 14 | claude-sonnet-4-6 | 1 | 264 | 55,100 | $0.0229 | 3.6s |
| 15 | claude-sonnet-4-6 | 1 | 569 | 55,735 | $0.0273 | 6.9s |
| 16 | claude-sonnet-4-6 | 1 | 918 | 56,270 | $0.0331 | 10.8s |
| 17 | claude-sonnet-4-6 | 1 | 172 | 56,932 | $0.0235 | 3.0s |
| 18 | claude-sonnet-4-6 | 1 | 194 | 58,861 | $0.0228 | 4.6s |
| 19 | claude-sonnet-4-6 | 1 | 182 | 59,464 | $0.0215 | 3.7s |
| 20 | claude-sonnet-4-6 | 1 | 111 | 59,700 | $0.0209 | 3.1s |
| 21 | claude-sonnet-4-6 | 1 | 75 | 60,166 | $0.0199 | 2.4s |
| 22 | claude-sonnet-4-6 | 1 | 213 | 60,487 | $0.0218 | 3.4s |
| 23 | claude-sonnet-4-6 | 1 | 192 | 60,598 | $0.0226 | 3.2s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
