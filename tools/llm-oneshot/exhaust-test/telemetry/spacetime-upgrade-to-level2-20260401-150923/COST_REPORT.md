# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-04-01
**Started:** 2026-04-01T15:09:23-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 27 |
| Total output tokens     | 13,011 |
| Total tokens            | 13,038 |
| Cache read tokens       | 1,486,359 |
| Cache creation tokens   | 25,510 |
| Total cost (USD)        | $0.7368 |
| Total API time          | 212.0s |
| API calls               | 27 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 214 | 31,321 | $0.0141 | 3.4s |
| 2 | claude-sonnet-4-6 | 1 | 170 | 31,321 | $0.0209 | 4.4s |
| 3 | claude-sonnet-4-6 | 1 | 295 | 33,717 | $0.0171 | 6.0s |
| 4 | claude-sonnet-4-6 | 1 | 4,185 | 42,400 | $0.0998 | 63.7s |
| 5 | claude-sonnet-4-6 | 1 | 199 | 48,880 | $0.0335 | 3.3s |
| 6 | claude-sonnet-4-6 | 1 | 210 | 53,098 | $0.0200 | 5.4s |
| 7 | claude-sonnet-4-6 | 1 | 210 | 53,339 | $0.0203 | 3.7s |
| 8 | claude-sonnet-4-6 | 1 | 557 | 53,647 | $0.0256 | 7.0s |
| 9 | claude-sonnet-4-6 | 1 | 199 | 53,955 | $0.0216 | 3.5s |
| 10 | claude-sonnet-4-6 | 1 | 222 | 53,955 | $0.0229 | 4.3s |
| 11 | claude-sonnet-4-6 | 1 | 1,660 | 54,851 | $0.0426 | 19.6s |
| 12 | claude-sonnet-4-6 | 1 | 199 | 55,171 | $0.0261 | 4.9s |
| 13 | claude-sonnet-4-6 | 1 | 316 | 56,910 | $0.0227 | 4.7s |
| 14 | claude-sonnet-4-6 | 1 | 291 | 57,151 | $0.0230 | 4.0s |
| 15 | claude-sonnet-4-6 | 1 | 326 | 57,546 | $0.0235 | 4.9s |
| 16 | claude-sonnet-4-6 | 1 | 415 | 57,916 | $0.0251 | 6.1s |
| 17 | claude-sonnet-4-6 | 1 | 642 | 58,321 | $0.0290 | 8.9s |
| 18 | claude-sonnet-4-6 | 1 | 1,097 | 58,815 | $0.0368 | 16.9s |
| 19 | claude-sonnet-4-6 | 1 | 159 | 59,536 | $0.0254 | 3.4s |
| 20 | claude-sonnet-4-6 | 1 | 199 | 62,188 | $0.0239 | 4.0s |
| 21 | claude-sonnet-4-6 | 1 | 172 | 62,792 | $0.0223 | 4.3s |
| 22 | claude-sonnet-4-6 | 1 | 255 | 63,033 | $0.0261 | 5.2s |
| 23 | claude-sonnet-4-6 | 1 | 110 | 64,885 | $0.0221 | 3.6s |
| 24 | claude-sonnet-4-6 | 1 | 184 | 65,136 | $0.0228 | 4.6s |
| 25 | claude-sonnet-4-6 | 1 | 158 | 65,269 | $0.0229 | 3.7s |
| 26 | claude-sonnet-4-6 | 1 | 142 | 65,515 | $0.0224 | 2.9s |
| 27 | claude-sonnet-4-6 | 1 | 225 | 65,691 | $0.0244 | 5.9s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
