# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-04-03
**Started:** 2026-04-03T16:24:26-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 32 |
| Total output tokens     | 9,961 |
| Total tokens            | 9,993 |
| Cache read tokens       | 2,203,611 |
| Cache creation tokens   | 22,964 |
| Total cost (USD)        | $0.8967 |
| Total API time          | 171.9s |
| API calls               | 32 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 144 | 47,014 | $0.0174 | 2.6s |
| 2 | claude-sonnet-4-6 | 1 | 161 | 47,306 | $0.0221 | 3.0s |
| 3 | claude-sonnet-4-6 | 1 | 161 | 48,763 | $0.0253 | 4.9s |
| 4 | claude-sonnet-4-6 | 1 | 161 | 50,951 | $0.0285 | 5.4s |
| 5 | claude-sonnet-4-6 | 1 | 161 | 56,816 | $0.0356 | 5.3s |
| 6 | claude-sonnet-4-6 | 1 | 737 | 65,001 | $0.0325 | 11.8s |
| 7 | claude-sonnet-4-6 | 1 | 202 | 65,508 | $0.0256 | 4.0s |
| 8 | claude-sonnet-4-6 | 1 | 309 | 66,280 | $0.0254 | 3.8s |
| 9 | claude-sonnet-4-6 | 1 | 202 | 66,524 | $0.0246 | 2.6s |
| 10 | claude-sonnet-4-6 | 1 | 300 | 66,945 | $0.0255 | 5.1s |
| 11 | claude-sonnet-4-6 | 1 | 635 | 67,189 | $0.0312 | 7.2s |
| 12 | claude-sonnet-4-6 | 1 | 474 | 67,601 | $0.0302 | 5.5s |
| 13 | claude-sonnet-4-6 | 1 | 698 | 68,348 | $0.0331 | 8.2s |
| 14 | claude-sonnet-4-6 | 1 | 548 | 68,915 | $0.0319 | 7.5s |
| 15 | claude-sonnet-4-6 | 1 | 221 | 69,706 | $0.0266 | 11.7s |
| 16 | claude-sonnet-4-6 | 1 | 228 | 70,347 | $0.0255 | 3.8s |
| 17 | claude-sonnet-4-6 | 1 | 344 | 70,347 | $0.0285 | 6.3s |
| 18 | claude-sonnet-4-6 | 1 | 446 | 70,931 | $0.0296 | 5.8s |
| 19 | claude-sonnet-4-6 | 1 | 606 | 71,368 | $0.0325 | 8.7s |
| 20 | claude-sonnet-4-6 | 1 | 518 | 72,606 | $0.0320 | 7.3s |
| 21 | claude-sonnet-4-6 | 1 | 764 | 73,264 | $0.0365 | 9.4s |
| 22 | claude-sonnet-4-6 | 1 | 307 | 74,075 | $0.0300 | 4.9s |
| 23 | claude-sonnet-4-6 | 1 | 202 | 74,932 | $0.0270 | 3.7s |
| 24 | claude-sonnet-4-6 | 1 | 209 | 75,332 | $0.0267 | 4.4s |
| 25 | claude-sonnet-4-6 | 1 | 125 | 76,424 | $0.0259 | 4.7s |
| 26 | claude-sonnet-4-6 | 1 | 222 | 76,723 | $0.0269 | 4.7s |
| 27 | claude-sonnet-4-6 | 1 | 96 | 78,217 | $0.0256 | 2.9s |
| 28 | claude-sonnet-4-6 | 1 | 210 | 78,400 | $0.0271 | 3.1s |
| 29 | claude-sonnet-4-6 | 1 | 173 | 78,918 | $0.0269 | 4.0s |
| 30 | claude-sonnet-4-6 | 1 | 170 | 79,078 | $0.0270 | 3.0s |
| 31 | claude-sonnet-4-6 | 1 | 219 | 79,716 | $0.0285 | 4.8s |
| 32 | claude-sonnet-4-6 | 1 | 8 | 80,066 | $0.0251 | 1.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
