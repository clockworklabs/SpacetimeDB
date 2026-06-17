# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-06-17
**Started:** 2026-06-17T15:44:37-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 2,082 |
| Total output tokens     | 48,656 |
| Total tokens            | 50,738 |
| Cache read tokens       | 1,695,631 |
| Cache creation tokens   | 71,255 |
| Total cost (USD)        | $1.6680 |
| Total API time          | 683.5s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-haiku-4-5-20251001 | 2,055 | 19 | 0 | $0.0022 | 1.7s |
| 2 | claude-sonnet-4-6 | 3 | 20,024 | 20,621 | $0.3990 | 285.8s |
| 3 | claude-sonnet-4-6 | 1 | 175 | 36,030 | $0.1339 | 5.7s |
| 4 | claude-sonnet-4-6 | 1 | 439 | 56,099 | $0.0251 | 5.3s |
| 5 | claude-sonnet-4-6 | 1 | 648 | 56,382 | $0.0307 | 8.6s |
| 6 | claude-sonnet-4-6 | 1 | 1,951 | 57,063 | $0.0509 | 20.7s |
| 7 | claude-sonnet-4-6 | 1 | 173 | 57,816 | $0.0323 | 3.2s |
| 8 | claude-sonnet-4-6 | 1 | 188 | 59,872 | $0.0227 | 3.7s |
| 9 | claude-sonnet-4-6 | 1 | 772 | 60,196 | $0.0310 | 13.5s |
| 10 | claude-sonnet-4-6 | 1 | 188 | 60,430 | $0.0263 | 2.9s |
| 11 | claude-sonnet-4-6 | 1 | 181 | 61,328 | $0.0229 | 3.5s |
| 12 | claude-sonnet-4-6 | 1 | 187 | 61,617 | $0.0225 | 5.8s |
| 13 | claude-sonnet-4-6 | 1 | 301 | 61,811 | $0.0264 | 5.4s |
| 14 | claude-sonnet-4-6 | 1 | 235 | 62,366 | $0.0433 | 5.5s |
| 15 | claude-sonnet-4-6 | 1 | 583 | 65,875 | $0.0359 | 9.3s |
| 16 | claude-sonnet-4-6 | 1 | 11,723 | 67,102 | $0.2019 | 158.3s |
| 17 | claude-sonnet-4-6 | 1 | 176 | 68,087 | $0.0967 | 3.7s |
| 18 | claude-sonnet-4-6 | 1 | 362 | 80,362 | $0.0312 | 5.4s |
| 19 | claude-sonnet-4-6 | 1 | 4,743 | 80,638 | $0.0981 | 56.1s |
| 20 | claude-sonnet-4-6 | 1 | 4,247 | 81,100 | $0.1171 | 45.7s |
| 21 | claude-sonnet-4-6 | 1 | 170 | 85,943 | $0.0544 | 3.6s |
| 22 | claude-sonnet-4-6 | 1 | 167 | 90,290 | $0.0309 | 7.4s |
| 23 | claude-sonnet-4-6 | 1 | 202 | 90,513 | $0.0313 | 3.3s |
| 24 | claude-sonnet-4-6 | 1 | 104 | 90,698 | $0.0318 | 2.7s |
| 25 | claude-sonnet-4-6 | 1 | 375 | 91,595 | $0.0343 | 8.0s |
| 26 | claude-sonnet-4-6 | 1 | 323 | 91,797 | $0.0352 | 8.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
