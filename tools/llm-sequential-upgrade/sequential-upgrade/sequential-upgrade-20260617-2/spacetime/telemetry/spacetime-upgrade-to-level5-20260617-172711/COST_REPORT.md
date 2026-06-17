# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-06-17
**Started:** 2026-06-17T17:27:11-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,413 |
| Total output tokens     | 16,755 |
| Total tokens            | 18,168 |
| Cache read tokens       | 1,944,029 |
| Cache creation tokens   | 64,726 |
| Total cost (USD)        | $1.2242 |
| Total API time          | 252.8s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,383 | 16 | 0 | $0.0015 | 1.1s |
| 2 | claude-sonnet-4-6 | 3 | 378 | 20,621 | $0.1007 | 7.4s |
| 3 | claude-sonnet-4-6 | 1 | 2,235 | 35,906 | $0.1278 | 36.8s |
| 4 | claude-sonnet-4-6 | 1 | 307 | 49,818 | $0.0859 | 6.0s |
| 5 | claude-sonnet-4-6 | 1 | 2,168 | 60,872 | $0.0612 | 32.6s |
| 6 | claude-sonnet-4-6 | 1 | 244 | 62,613 | $0.0362 | 5.2s |
| 7 | claude-sonnet-4-6 | 1 | 478 | 64,907 | $0.0289 | 7.0s |
| 8 | claude-sonnet-4-6 | 1 | 217 | 65,277 | $0.0264 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 797 | 65,862 | $0.0373 | 11.4s |
| 10 | claude-sonnet-4-6 | 1 | 280 | 66,795 | $0.0297 | 3.9s |
| 11 | claude-sonnet-4-6 | 1 | 314 | 67,699 | $0.0279 | 4.3s |
| 12 | claude-sonnet-4-6 | 1 | 192 | 68,185 | $0.0259 | 3.5s |
| 13 | claude-sonnet-4-6 | 1 | 545 | 68,606 | $0.0311 | 9.3s |
| 14 | claude-sonnet-4-6 | 1 | 164 | 69,003 | $0.0274 | 2.6s |
| 15 | claude-sonnet-4-6 | 1 | 373 | 69,714 | $0.0281 | 5.8s |
| 16 | claude-sonnet-4-6 | 1 | 288 | 69,974 | $0.0284 | 4.4s |
| 17 | claude-sonnet-4-6 | 1 | 253 | 70,492 | $0.0310 | 3.5s |
| 18 | claude-sonnet-4-6 | 1 | 1,105 | 71,509 | $0.0690 | 20.0s |
| 19 | claude-sonnet-4-6 | 1 | 452 | 76,667 | $0.0368 | 5.5s |
| 20 | claude-sonnet-4-6 | 1 | 462 | 77,839 | $0.0336 | 6.2s |
| 21 | claude-sonnet-4-6 | 1 | 346 | 78,393 | $0.0345 | 6.1s |
| 22 | claude-sonnet-4-6 | 1 | 498 | 79,360 | $0.0340 | 6.7s |
| 23 | claude-sonnet-4-6 | 1 | 2,614 | 79,808 | $0.0668 | 29.2s |
| 24 | claude-sonnet-4-6 | 1 | 1,312 | 80,408 | $0.0607 | 16.1s |
| 25 | claude-sonnet-4-6 | 1 | 181 | 83,223 | $0.0362 | 3.6s |
| 26 | claude-sonnet-4-6 | 1 | 174 | 84,637 | $0.0292 | 3.3s |
| 27 | claude-sonnet-4-6 | 1 | 221 | 84,836 | $0.0316 | 3.8s |
| 28 | claude-sonnet-4-6 | 1 | 120 | 85,310 | $0.0297 | 2.4s |
| 29 | claude-sonnet-4-6 | 1 | 21 | 85,695 | $0.0268 | 1.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
