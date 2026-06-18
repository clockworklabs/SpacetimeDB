# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 5
**Date:** 2026-06-18
**Started:** 2026-06-18T11:19:25-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 1,479 |
| Total output tokens     | 10,571 |
| Total tokens            | 12,050 |
| Cache read tokens       | 1,588,855 |
| Cache creation tokens   | 28,656 |
| Total cost (USD)        | $0.7442 |
| Total API time          | 161.5s |
| API calls               | 27 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 1,381 | 16 | 0 | $0.0015 | 1.5s |
| 2 | claude-sonnet-4-6 | 3 | 358 | 20,621 | $0.0680 | 6.0s |
| 3 | claude-sonnet-4-6 | 71 | 206 | 35,675 | $0.0157 | 3.3s |
| 4 | claude-sonnet-4-6 | 1 | 775 | 55,976 | $0.0294 | 9.6s |
| 5 | claude-sonnet-4-6 | 1 | 785 | 56,236 | $0.0320 | 9.3s |
| 6 | claude-sonnet-4-6 | 1 | 198 | 58,044 | $0.0209 | 3.5s |
| 7 | claude-sonnet-4-6 | 1 | 350 | 58,179 | $0.0242 | 12.9s |
| 8 | claude-sonnet-4-6 | 1 | 191 | 58,568 | $0.0222 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 259 | 59,042 | $0.0232 | 3.9s |
| 10 | claude-sonnet-4-6 | 1 | 205 | 59,478 | $0.0243 | 4.8s |
| 11 | claude-sonnet-4-6 | 1 | 272 | 61,015 | $0.0231 | 6.5s |
| 12 | claude-sonnet-4-6 | 1 | 584 | 61,204 | $0.0285 | 7.6s |
| 13 | claude-sonnet-4-6 | 1 | 672 | 61,576 | $0.0315 | 8.3s |
| 14 | claude-sonnet-4-6 | 1 | 2,199 | 62,359 | $0.0546 | 24.0s |
| 15 | claude-sonnet-4-6 | 1 | 346 | 63,131 | $0.0328 | 5.2s |
| 16 | claude-sonnet-4-6 | 1 | 226 | 65,430 | $0.0247 | 3.4s |
| 17 | claude-sonnet-4-6 | 1 | 335 | 65,876 | $0.0260 | 4.6s |
| 18 | claude-sonnet-4-6 | 1 | 339 | 66,202 | $0.0266 | 6.6s |
| 19 | claude-sonnet-4-6 | 1 | 171 | 66,637 | $0.0246 | 3.5s |
| 20 | claude-sonnet-4-6 | 1 | 152 | 67,190 | $0.0245 | 2.7s |
| 21 | claude-sonnet-4-6 | 1 | 337 | 67,728 | $0.0265 | 5.5s |
| 22 | claude-sonnet-4-6 | 1 | 809 | 68,476 | $0.0347 | 11.2s |
| 23 | claude-sonnet-4-6 | 1 | 176 | 69,023 | $0.0265 | 3.2s |
| 24 | claude-sonnet-4-6 | 1 | 160 | 69,850 | $0.0241 | 2.9s |
| 25 | claude-sonnet-4-6 | 1 | 167 | 70,044 | $0.0252 | 3.1s |
| 26 | claude-sonnet-4-6 | 1 | 161 | 70,505 | $0.0246 | 2.7s |
| 27 | claude-sonnet-4-6 | 1 | 122 | 70,790 | $0.0243 | 2.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
