# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T16:09:37-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 26 |
| Total output tokens     | 5,419 |
| Total tokens            | 5,445 |
| Cache read tokens       | 1,137,446 |
| Cache creation tokens   | 19,028 |
| Total cost (USD)        | $0.4940 |
| Total API time          | 102.4s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 199 | 30,352 | $0.0134 | 2.6s |
| 2 | claude-sonnet-4-6 | 1 | 155 | 30,708 | $0.0158 | 3.1s |
| 3 | claude-sonnet-4-6 | 1 | 155 | 31,856 | $0.0162 | 3.5s |
| 4 | claude-sonnet-4-6 | 1 | 155 | 33,007 | $0.0215 | 3.6s |
| 5 | claude-sonnet-4-6 | 1 | 155 | 35,478 | $0.0217 | 3.7s |
| 6 | claude-sonnet-4-6 | 1 | 405 | 37,808 | $0.0259 | 13.5s |
| 7 | claude-sonnet-4-6 | 1 | 155 | 41,921 | $0.0158 | 2.2s |
| 8 | claude-sonnet-4-6 | 1 | 338 | 42,157 | $0.0202 | 5.0s |
| 9 | claude-sonnet-4-6 | 1 | 268 | 42,822 | $0.0222 | 5.6s |
| 10 | claude-sonnet-4-6 | 1 | 195 | 44,548 | $0.0182 | 2.6s |
| 11 | claude-sonnet-4-6 | 1 | 172 | 45,068 | $0.0170 | 2.6s |
| 12 | claude-sonnet-4-6 | 1 | 449 | 45,305 | $0.0220 | 6.7s |
| 13 | claude-sonnet-4-6 | 1 | 277 | 45,752 | $0.0200 | 3.5s |
| 14 | claude-sonnet-4-6 | 1 | 257 | 46,307 | $0.0191 | 6.6s |
| 15 | claude-sonnet-4-6 | 1 | 304 | 46,671 | $0.0199 | 4.8s |
| 16 | claude-sonnet-4-6 | 1 | 195 | 47,015 | $0.0185 | 7.5s |
| 17 | claude-sonnet-4-6 | 1 | 164 | 47,406 | $0.0176 | 2.6s |
| 18 | claude-sonnet-4-6 | 1 | 150 | 47,643 | $0.0172 | 2.1s |
| 19 | claude-sonnet-4-6 | 1 | 140 | 47,825 | $0.0171 | 2.7s |
| 20 | claude-sonnet-4-6 | 1 | 98 | 48,174 | $0.0167 | 1.7s |
| 21 | claude-sonnet-4-6 | 1 | 108 | 48,739 | $0.0177 | 1.7s |
| 22 | claude-sonnet-4-6 | 1 | 211 | 49,123 | $0.0184 | 2.7s |
| 23 | claude-sonnet-4-6 | 1 | 154 | 49,505 | $0.0204 | 2.1s |
| 24 | claude-sonnet-4-6 | 1 | 154 | 49,505 | $0.0219 | 2.0s |
| 25 | claude-sonnet-4-6 | 1 | 193 | 51,142 | $0.0200 | 3.1s |
| 26 | claude-sonnet-4-6 | 1 | 213 | 51,609 | $0.0196 | 4.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
