# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-04T13:45:35-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 34 |
| Total output tokens     | 20,098 |
| Total tokens            | 20,132 |
| Cache read tokens       | 2,131,170 |
| Cache creation tokens   | 48,104 |
| Total cost (USD)        | $1.1213 |
| Total API time          | 374.2s |
| API calls               | 32 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 159 | 20,668 | $0.0434 | 4.3s |
| 2 | claude-sonnet-4-6 | 1 | 270 | 30,737 | $0.0197 | 6.2s |
| 3 | claude-sonnet-4-6 | 1 | 361 | 32,440 | $0.0196 | 7.0s |
| 4 | claude-sonnet-4-6 | 1 | 161 | 33,620 | $0.0187 | 6.1s |
| 5 | claude-sonnet-4-6 | 1 | 161 | 37,628 | $0.0155 | 2.5s |
| 6 | claude-sonnet-4-6 | 1 | 368 | 38,118 | $0.0197 | 6.3s |
| 7 | claude-sonnet-4-6 | 1 | 161 | 40,444 | $0.0166 | 2.8s |
| 8 | claude-sonnet-4-6 | 1 | 727 | 40,981 | $0.0278 | 12.3s |
| 9 | claude-sonnet-4-6 | 1 | 437 | 42,203 | $0.0264 | 8.5s |
| 10 | claude-sonnet-4-6 | 1 | 161 | 44,115 | $0.0187 | 4.8s |
| 11 | claude-sonnet-4-6 | 1 | 414 | 44,934 | $0.0234 | 8.9s |
| 12 | claude-sonnet-4-6 | 1 | 360 | 45,927 | $0.0266 | 6.6s |
| 13 | claude-sonnet-4-6 | 1 | 373 | 48,776 | $0.0226 | 7.5s |
| 14 | claude-sonnet-4-6 | 1 | 476 | 49,410 | $0.0278 | 9.5s |
| 15 | claude-sonnet-4-6 | 1 | 697 | 50,972 | $0.0305 | 13.0s |
| 16 | claude-sonnet-4-6 | 1 | 228 | 52,242 | $0.0262 | 4.1s |
| 17 | claude-sonnet-4-6 | 1 | 1,923 | 55,256 | $0.0505 | 30.6s |
| 18 | claude-sonnet-4-6 | 1 | 205 | 56,610 | $0.0349 | 5.8s |
| 19 | claude-sonnet-4-6 | 1 | 196 | 60,570 | $0.0236 | 3.1s |
| 20 | claude-sonnet-4-6 | 1 | 2,117 | 61,957 | $0.0617 | 36.7s |
| 21 | claude-sonnet-4-6 | 1 | 248 | 67,308 | $0.0263 | 7.1s |
| 22 | claude-sonnet-4-6 | 1 | 848 | 80,513 | $0.0451 | 14.9s |
| 23 | claude-sonnet-4-6 | 1 | 161 | 101,734 | $0.0373 | 3.9s |
| 24 | claude-sonnet-4-6 | 1 | 466 | 101,734 | $0.0450 | 9.5s |
| 25 | claude-sonnet-4-6 | 1 | 417 | 103,721 | $0.0412 | 8.0s |
| 26 | claude-sonnet-4-6 | 1 | 161 | 104,734 | $0.0358 | 3.8s |
| 27 | claude-sonnet-4-6 | 1 | 6,170 | 105,960 | $0.1290 | 101.8s |
| 28 | claude-sonnet-4-6 | 1 | 194 | 113,918 | $0.0403 | 6.4s |
| 29 | claude-sonnet-4-6 | 1 | 334 | 114,766 | $0.0410 | 6.6s |
| 30 | claude-sonnet-4-6 | 1 | 181 | 115,190 | $0.0389 | 4.4s |
| 31 | claude-sonnet-4-6 | 1 | 185 | 116,821 | $0.0391 | 5.7s |
| 32 | claude-sonnet-4-6 | 1 | 778 | 117,163 | $0.0485 | 15.8s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
