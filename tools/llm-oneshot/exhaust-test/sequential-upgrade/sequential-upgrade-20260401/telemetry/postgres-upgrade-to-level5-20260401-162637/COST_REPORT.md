# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 5
**Date:** 2026-04-01
**Started:** 2026-04-01T16:26:37-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 40 |
| Total output tokens     | 17,478 |
| Total tokens            | 17,518 |
| Cache read tokens       | 2,567,374 |
| Cache creation tokens   | 34,761 |
| Total cost (USD)        | $1.1629 |
| Total API time          | 315.9s |
| API calls               | 40 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 765 | 56,676 | $0.0292 | 11.1s |
| 2 | claude-sonnet-4-6 | 1 | 366 | 56,866 | $0.0257 | 6.0s |
| 3 | claude-sonnet-4-6 | 1 | 1,188 | 65,783 | $0.0428 | 11.8s |
| 4 | claude-sonnet-4-6 | 1 | 196 | 57,711 | $0.0219 | 4.3s |
| 5 | claude-sonnet-4-6 | 1 | 255 | 67,194 | $0.0296 | 4.0s |
| 6 | claude-sonnet-4-6 | 1 | 287 | 35,082 | $0.0167 | 3.8s |
| 7 | claude-sonnet-4-6 | 1 | 201 | 31,321 | $0.0143 | 3.7s |
| 8 | claude-sonnet-4-6 | 1 | 352 | 58,157 | $0.0236 | 4.5s |
| 9 | claude-sonnet-4-6 | 1 | 329 | 58,395 | $0.0241 | 4.9s |
| 10 | claude-sonnet-4-6 | 1 | 391 | 58,827 | $0.0250 | 5.0s |
| 11 | claude-sonnet-4-6 | 1 | 571 | 59,236 | $0.0281 | 8.7s |
| 12 | claude-sonnet-4-6 | 1 | 255 | 69,274 | $0.0275 | 4.1s |
| 13 | claude-sonnet-4-6 | 1 | 171 | 70,056 | $0.0247 | 3.8s |
| 14 | claude-sonnet-4-6 | 1 | 176 | 70,353 | $0.0245 | 3.7s |
| 15 | claude-sonnet-4-6 | 1 | 113 | 70,542 | $0.0246 | 3.4s |
| 16 | claude-sonnet-4-6 | 1 | 1,769 | 59,707 | $0.0469 | 18.8s |
| 17 | claude-sonnet-4-6 | 1 | 233 | 56,989 | $0.0232 | 7.7s |
| 18 | claude-sonnet-4-6 | 1 | 514 | 60,358 | $0.0328 | 9.3s |
| 19 | claude-sonnet-4-6 | 1 | 104 | 71,017 | $0.0241 | 2.7s |
| 20 | claude-sonnet-4-6 | 1 | 158 | 62,207 | $0.0239 | 3.5s |
| 21 | claude-sonnet-4-6 | 1 | 1,779 | 56,272 | $0.0653 | 27.1s |
| 22 | claude-sonnet-4-6 | 1 | 253 | 71,350 | $0.0256 | 4.7s |
| 23 | claude-sonnet-4-6 | 1 | 504 | 62,079 | $0.0330 | 8.2s |
| 24 | claude-sonnet-4-6 | 1 | 180 | 71,467 | $0.0252 | 6.3s |
| 25 | claude-sonnet-4-6 | 1 | 147 | 63,897 | $0.0236 | 3.7s |
| 26 | claude-sonnet-4-6 | 1 | 169 | 64,196 | $0.0254 | 4.1s |
| 27 | claude-sonnet-4-6 | 1 | 193 | 64,865 | $0.0384 | 6.0s |
| 28 | claude-sonnet-4-6 | 1 | 175 | 66,075 | $0.0237 | 3.4s |
| 29 | claude-sonnet-4-6 | 1 | 642 | 70,247 | $0.0323 | 11.8s |
| 30 | claude-sonnet-4-6 | 1 | 159 | 66,398 | $0.0234 | 2.9s |
| 31 | claude-sonnet-4-6 | 1 | 111 | 66,699 | $0.0223 | 3.6s |
| 32 | claude-sonnet-4-6 | 1 | 323 | 71,379 | $0.0277 | 8.4s |
| 33 | claude-sonnet-4-6 | 1 | 189 | 66,876 | $0.0234 | 4.3s |
| 34 | claude-sonnet-4-6 | 1 | 146 | 67,009 | $0.0233 | 2.9s |
| 35 | claude-sonnet-4-6 | 1 | 226 | 67,009 | $0.0252 | 7.3s |
| 36 | claude-sonnet-4-6 | 1 | 635 | 72,748 | $0.0347 | 18.1s |
| 37 | claude-sonnet-4-6 | 1 | 147 | 74,316 | $0.0258 | 3.3s |
| 38 | claude-sonnet-4-6 | 1 | 2,325 | 74,664 | $0.0617 | 47.9s |
| 39 | claude-sonnet-4-6 | 1 | 612 | 75,836 | $0.0410 | 13.2s |
| 40 | claude-sonnet-4-6 | 1 | 169 | 78,241 | $0.0286 | 4.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
