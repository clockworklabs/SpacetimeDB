# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-04
**Started:** 2026-04-03T21:18:04-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 26 |
| Total output tokens     | 6,606 |
| Total tokens            | 6,632 |
| Cache read tokens       | 1,147,458 |
| Cache creation tokens   | 16,637 |
| Total cost (USD)        | $0.5058 |
| Total API time          | 105.3s |
| API calls               | 26 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 1 | 305 | 32,961 | $0.0212 | 3.5s |
| 2 | claude-sonnet-4-6 | 1 | 141 | 36,025 | $0.0158 | 2.5s |
| 3 | claude-sonnet-4-6 | 1 | 228 | 36,970 | $0.0161 | 4.4s |
| 4 | claude-sonnet-4-6 | 1 | 161 | 37,400 | $0.0223 | 2.6s |
| 5 | claude-sonnet-4-6 | 1 | 182 | 40,461 | $0.0164 | 2.7s |
| 6 | claude-sonnet-4-6 | 1 | 161 | 40,461 | $0.0170 | 2.9s |
| 7 | claude-sonnet-4-6 | 1 | 306 | 41,113 | $0.0193 | 5.3s |
| 8 | claude-sonnet-4-6 | 1 | 227 | 41,751 | $0.0172 | 3.6s |
| 9 | claude-sonnet-4-6 | 1 | 350 | 41,751 | $0.0204 | 3.9s |
| 10 | claude-sonnet-4-6 | 1 | 251 | 42,438 | $0.0182 | 3.1s |
| 11 | claude-sonnet-4-6 | 1 | 591 | 42,438 | $0.0244 | 6.7s |
| 12 | claude-sonnet-4-6 | 1 | 186 | 43,193 | $0.0184 | 4.5s |
| 13 | claude-sonnet-4-6 | 1 | 177 | 43,896 | $0.0171 | 3.2s |
| 14 | claude-sonnet-4-6 | 1 | 361 | 44,239 | $0.0212 | 5.5s |
| 15 | claude-sonnet-4-6 | 1 | 251 | 45,771 | $0.0195 | 6.5s |
| 16 | claude-sonnet-4-6 | 1 | 247 | 46,308 | $0.0187 | 3.6s |
| 17 | claude-sonnet-4-6 | 1 | 269 | 46,601 | $0.0193 | 4.6s |
| 18 | claude-sonnet-4-6 | 1 | 307 | 46,941 | $0.0200 | 4.5s |
| 19 | claude-sonnet-4-6 | 1 | 254 | 47,303 | $0.0195 | 3.5s |
| 20 | claude-sonnet-4-6 | 1 | 553 | 47,703 | $0.0239 | 7.6s |
| 21 | claude-sonnet-4-6 | 1 | 251 | 48,050 | $0.0214 | 3.5s |
| 22 | claude-sonnet-4-6 | 1 | 192 | 48,895 | $0.0187 | 3.3s |
| 23 | claude-sonnet-4-6 | 1 | 131 | 49,188 | $0.0181 | 3.6s |
| 24 | claude-sonnet-4-6 | 1 | 115 | 49,709 | $0.0174 | 2.9s |
| 25 | claude-sonnet-4-6 | 1 | 160 | 51,688 | $0.0212 | 2.7s |
| 26 | claude-sonnet-4-6 | 1 | 249 | 54,204 | $0.0229 | 4.6s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
