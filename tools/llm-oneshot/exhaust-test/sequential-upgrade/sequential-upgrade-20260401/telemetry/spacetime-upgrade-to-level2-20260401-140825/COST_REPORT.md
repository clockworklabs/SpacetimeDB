# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 2
**Date:** 2026-04-01
**Started:** 2026-04-01T14:08:25-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 185 |
| Total output tokens     | 11,572 |
| Total tokens            | 11,757 |
| Cache read tokens       | 1,754,369 |
| Cache creation tokens   | 24,350 |
| Total cost (USD)        | $0.7918 |
| Total API time          | 193.9s |
| API calls               | 30 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 187 | 35,082 | $0.0151 | 3.5s |
| 2 | claude-sonnet-4-6 | 1 | 299 | 35,557 | $0.0196 | 5.0s |
| 3 | claude-sonnet-4-6 | 156 | 527 | 47,229 | $0.0293 | 8.3s |
| 4 | claude-sonnet-4-6 | 1 | 231 | 49,043 | $0.0209 | 4.4s |
| 5 | claude-sonnet-4-6 | 1 | 500 | 49,758 | $0.0237 | 8.2s |
| 6 | claude-sonnet-4-6 | 1 | 574 | 50,093 | $0.0259 | 7.7s |
| 7 | claude-sonnet-4-6 | 1 | 664 | 50,696 | $0.0277 | 9.2s |
| 8 | claude-sonnet-4-6 | 1 | 248 | 51,374 | $0.0219 | 4.8s |
| 9 | claude-sonnet-4-6 | 1 | 161 | 52,123 | $0.0193 | 4.1s |
| 10 | claude-sonnet-4-6 | 1 | 1,102 | 52,456 | $0.0350 | 10.9s |
| 11 | claude-sonnet-4-6 | 1 | 252 | 53,191 | $0.0242 | 5.9s |
| 12 | claude-sonnet-4-6 | 1 | 146 | 54,378 | $0.0196 | 3.2s |
| 13 | claude-sonnet-4-6 | 1 | 262 | 54,853 | $0.0234 | 4.0s |
| 14 | claude-sonnet-4-6 | 1 | 279 | 55,969 | $0.0245 | 4.8s |
| 15 | claude-sonnet-4-6 | 1 | 119 | 56,919 | $0.0224 | 3.6s |
| 16 | claude-sonnet-4-6 | 1 | 482 | 56,919 | $0.0410 | 10.4s |
| 17 | claude-sonnet-4-6 | 1 | 129 | 62,856 | $0.0239 | 3.1s |
| 18 | claude-sonnet-4-6 | 1 | 578 | 63,691 | $0.0306 | 13.2s |
| 19 | claude-sonnet-4-6 | 1 | 453 | 64,441 | $0.0285 | 6.6s |
| 20 | claude-sonnet-4-6 | 1 | 351 | 65,060 | $0.0269 | 5.8s |
| 21 | claude-sonnet-4-6 | 1 | 385 | 65,612 | $0.0271 | 5.0s |
| 22 | claude-sonnet-4-6 | 1 | 1,093 | 66,043 | $0.0380 | 14.2s |
| 23 | claude-sonnet-4-6 | 1 | 1,051 | 66,508 | $0.0401 | 13.1s |
| 24 | claude-sonnet-4-6 | 1 | 158 | 67,681 | $0.0269 | 4.5s |
| 25 | claude-sonnet-4-6 | 1 | 252 | 70,101 | $0.0273 | 4.3s |
| 26 | claude-sonnet-4-6 | 1 | 162 | 70,771 | $0.0248 | 3.7s |
| 27 | claude-sonnet-4-6 | 1 | 156 | 71,065 | $0.0243 | 3.3s |
| 28 | claude-sonnet-4-6 | 1 | 231 | 71,245 | $0.0266 | 6.5s |
| 29 | claude-sonnet-4-6 | 1 | 250 | 71,701 | $0.0262 | 3.9s |
| 30 | claude-sonnet-4-6 | 1 | 290 | 71,954 | $0.0270 | 9.0s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
