# Cost Report

**App:** chat-app
**Backend:** postgres
**Level:** 12
**Date:** 2026-04-10
**Started:** 2026-04-10T14:53:12-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 32 |
| Total output tokens     | 5,929 |
| Total tokens            | 5,961 |
| Cache read tokens       | 1,102,106 |
| Cache creation tokens   | 30,493 |
| Total cost (USD)        | $0.5340 |
| Total API time          | 103.0s |
| API calls               | 28 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 266 | 20,574 | $0.0341 | 3.3s |
| 2 | claude-sonnet-4-6 | 1 | 271 | 26,962 | $0.0167 | 3.6s |
| 3 | claude-sonnet-4-6 | 1 | 293 | 28,178 | $0.0143 | 3.6s |
| 4 | claude-sonnet-4-6 | 1 | 155 | 31,149 | $0.0149 | 2.5s |
| 5 | claude-sonnet-4-6 | 1 | 277 | 31,999 | $0.0182 | 4.1s |
| 6 | claude-sonnet-4-6 | 1 | 155 | 33,171 | $0.0187 | 2.8s |
| 7 | claude-sonnet-4-6 | 1 | 470 | 33,171 | $0.0275 | 5.3s |
| 8 | claude-sonnet-4-6 | 1 | 190 | 35,980 | $0.0158 | 2.9s |
| 9 | claude-sonnet-4-6 | 1 | 155 | 36,556 | $0.0155 | 2.5s |
| 10 | claude-sonnet-4-6 | 1 | 697 | 37,136 | $0.0250 | 8.9s |
| 11 | claude-sonnet-4-6 | 1 | 448 | 39,216 | $0.0196 | 7.4s |
| 12 | claude-sonnet-4-6 | 1 | 153 | 39,509 | $0.0159 | 2.8s |
| 13 | claude-sonnet-4-6 | 1 | 156 | 40,146 | $0.0154 | 3.0s |
| 14 | claude-sonnet-4-6 | 1 | 149 | 40,405 | $0.0150 | 2.8s |
| 15 | claude-sonnet-4-6 | 1 | 116 | 40,754 | $0.0157 | 2.6s |
| 16 | claude-sonnet-4-6 | 1 | 121 | 41,222 | $0.0150 | 2.1s |
| 17 | claude-sonnet-4-6 | 1 | 91 | 41,442 | $0.0149 | 3.1s |
| 18 | claude-sonnet-4-6 | 1 | 99 | 41,726 | $0.0149 | 2.0s |
| 19 | claude-sonnet-4-6 | 1 | 101 | 43,775 | $0.0155 | 2.2s |
| 20 | claude-sonnet-4-6 | 1 | 166 | 43,998 | $0.0166 | 2.5s |
| 21 | claude-sonnet-4-6 | 1 | 128 | 44,253 | $0.0163 | 2.2s |
| 22 | claude-sonnet-4-6 | 1 | 99 | 44,542 | $0.0159 | 2.0s |
| 23 | claude-sonnet-4-6 | 1 | 118 | 44,941 | $0.0159 | 2.2s |
| 24 | claude-sonnet-4-6 | 1 | 132 | 45,104 | $0.0170 | 3.9s |
| 25 | claude-sonnet-4-6 | 1 | 93 | 45,663 | $0.0159 | 2.0s |
| 26 | claude-sonnet-4-6 | 1 | 90 | 46,265 | $0.0161 | 1.8s |
| 27 | claude-sonnet-4-6 | 1 | 710 | 46,770 | $0.0590 | 16.7s |
| 28 | claude-sonnet-4-6 | 3 | 30 | 57,499 | $0.0189 | 2.4s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
