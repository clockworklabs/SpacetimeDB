# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-06
**Started:** 2026-04-06T15:37:27-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 31 |
| Total output tokens     | 19,972 |
| Total tokens            | 20,003 |
| Cache read tokens       | 1,347,954 |
| Cache creation tokens   | 35,829 |
| Total cost (USD)        | $0.8384 |
| Total API time          | 281.4s |
| API calls               | 29 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 514 | 20,510 | $0.0576 | 8.0s |
| 2 | claude-sonnet-4-6 | 1 | 83 | 32,165 | $0.0130 | 4.1s |
| 3 | claude-sonnet-4-6 | 1 | 262 | 32,727 | $0.0142 | 4.1s |
| 4 | claude-sonnet-4-6 | 1 | 206 | 33,157 | $0.0140 | 8.4s |
| 5 | claude-sonnet-4-6 | 1 | 236 | 33,401 | $0.0147 | 8.1s |
| 6 | claude-sonnet-4-6 | 1 | 881 | 33,701 | $0.0246 | 14.5s |
| 7 | claude-sonnet-4-6 | 1 | 2,619 | 34,032 | $0.0532 | 28.4s |
| 8 | claude-sonnet-4-6 | 1 | 179 | 35,009 | $0.0234 | 2.8s |
| 9 | claude-sonnet-4-6 | 1 | 1,299 | 37,956 | $0.0349 | 19.9s |
| 10 | claude-sonnet-4-6 | 1 | 256 | 40,638 | $0.0173 | 4.8s |
| 11 | claude-sonnet-4-6 | 1 | 186 | 41,264 | $0.0228 | 5.5s |
| 12 | claude-sonnet-4-6 | 1 | 246 | 46,631 | $0.0202 | 7.9s |
| 13 | claude-sonnet-4-6 | 1 | 403 | 47,314 | $0.0213 | 5.4s |
| 14 | claude-sonnet-4-6 | 1 | 199 | 47,602 | $0.0191 | 3.1s |
| 15 | claude-sonnet-4-6 | 1 | 335 | 48,094 | $0.0205 | 4.2s |
| 16 | claude-sonnet-4-6 | 1 | 254 | 48,385 | $0.0199 | 4.2s |
| 17 | claude-sonnet-4-6 | 1 | 182 | 48,810 | $0.0187 | 7.7s |
| 18 | claude-sonnet-4-6 | 1 | 392 | 48,810 | $0.0228 | 5.1s |
| 19 | claude-sonnet-4-6 | 1 | 3,798 | 49,426 | $0.0736 | 35.8s |
| 20 | claude-sonnet-4-6 | 1 | 5,514 | 49,909 | $0.1123 | 58.0s |
| 21 | claude-sonnet-4-6 | 1 | 175 | 53,798 | $0.0406 | 3.3s |
| 22 | claude-sonnet-4-6 | 1 | 246 | 59,615 | $0.0224 | 4.9s |
| 23 | claude-sonnet-4-6 | 1 | 173 | 59,843 | $0.0216 | 3.0s |
| 24 | claude-sonnet-4-6 | 1 | 174 | 60,131 | $0.0214 | 6.5s |
| 25 | claude-sonnet-4-6 | 1 | 253 | 60,322 | $0.0237 | 3.6s |
| 26 | claude-sonnet-4-6 | 1 | 215 | 60,797 | $0.0226 | 3.7s |
| 27 | claude-sonnet-4-6 | 1 | 122 | 61,092 | $0.0211 | 5.0s |
| 28 | claude-sonnet-4-6 | 1 | 254 | 61,340 | $0.0227 | 4.0s |
| 29 | claude-sonnet-4-6 | 1 | 316 | 61,475 | $0.0243 | 7.7s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
