# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 7
**Date:** 2026-04-02
**Started:** 2026-04-02T16:02:50-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 39 |
| Total output tokens     | 67,573 |
| Total tokens            | 67,612 |
| Cache read tokens       | 2,413,964 |
| Cache creation tokens   | 57,598 |
| Total cost (USD)        | $1.9539 |
| Total API time          | 910.3s |
| API calls               | 37 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-sonnet-4-6 | 3 | 196 | 20,668 | $0.0659 | 4.4s |
| 2 | claude-sonnet-4-6 | 1 | 32,000 | 35,807 | $0.4976 | 448.4s |
| 3 | claude-sonnet-4-6 | 1 | 249 | 37,685 | $0.0157 | 4.5s |
| 4 | claude-sonnet-4-6 | 1 | 208 | 38,153 | $0.0155 | 3.3s |
| 5 | claude-sonnet-4-6 | 1 | 238 | 38,401 | $0.0162 | 3.0s |
| 6 | claude-sonnet-4-6 | 1 | 1,504 | 38,401 | $0.0365 | 15.9s |
| 7 | claude-sonnet-4-6 | 1 | 4,259 | 39,040 | $0.0816 | 42.1s |
| 8 | claude-sonnet-4-6 | 1 | 240 | 40,642 | $0.0321 | 4.3s |
| 9 | claude-sonnet-4-6 | 1 | 173 | 44,999 | $0.0172 | 2.9s |
| 10 | claude-sonnet-4-6 | 1 | 4,202 | 45,507 | $0.0792 | 70.7s |
| 11 | claude-sonnet-4-6 | 1 | 5,487 | 46,183 | $0.1123 | 52.0s |
| 12 | claude-sonnet-4-6 | 1 | 250 | 56,065 | $0.0218 | 4.6s |
| 13 | claude-sonnet-4-6 | 1 | 307 | 60,009 | $0.0384 | 6.6s |
| 14 | claude-sonnet-4-6 | 1 | 240 | 64,226 | $0.0267 | 5.0s |
| 15 | claude-sonnet-4-6 | 1 | 405 | 65,246 | $0.0267 | 6.8s |
| 16 | claude-sonnet-4-6 | 1 | 337 | 66,024 | $0.0260 | 4.6s |
| 17 | claude-sonnet-4-6 | 1 | 256 | 66,319 | $0.0253 | 4.1s |
| 18 | claude-sonnet-4-6 | 1 | 184 | 66,748 | $0.0241 | 3.3s |
| 19 | claude-sonnet-4-6 | 1 | 394 | 67,095 | $0.0271 | 5.8s |
| 20 | claude-sonnet-4-6 | 1 | 4,365 | 67,372 | $0.0875 | 46.4s |
| 21 | claude-sonnet-4-6 | 1 | 8,409 | 67,859 | $0.1632 | 100.4s |
| 22 | claude-sonnet-4-6 | 1 | 168 | 72,317 | $0.0568 | 4.1s |
| 23 | claude-sonnet-4-6 | 1 | 240 | 81,007 | $0.0287 | 4.2s |
| 24 | claude-sonnet-4-6 | 1 | 175 | 81,228 | $0.0281 | 3.3s |
| 25 | claude-sonnet-4-6 | 1 | 143 | 81,912 | $0.0293 | 3.0s |
| 26 | claude-sonnet-4-6 | 1 | 417 | 82,600 | $0.0339 | 5.4s |
| 27 | claude-sonnet-4-6 | 1 | 162 | 83,364 | $0.0313 | 4.5s |
| 28 | claude-sonnet-4-6 | 1 | 227 | 83,364 | $0.0336 | 3.9s |
| 29 | claude-sonnet-4-6 | 1 | 287 | 84,751 | $0.0310 | 5.1s |
| 30 | claude-sonnet-4-6 | 1 | 219 | 85,091 | $0.0303 | 4.2s |
| 31 | claude-sonnet-4-6 | 1 | 176 | 85,491 | $0.0295 | 4.8s |
| 32 | claude-sonnet-4-6 | 1 | 177 | 85,804 | $0.0298 | 3.4s |
| 33 | claude-sonnet-4-6 | 1 | 247 | 86,187 | $0.0314 | 4.3s |
| 34 | claude-sonnet-4-6 | 1 | 92 | 86,665 | $0.0285 | 2.4s |
| 35 | claude-sonnet-4-6 | 1 | 123 | 87,066 | $0.0287 | 2.7s |
| 36 | claude-sonnet-4-6 | 1 | 238 | 87,266 | $0.0303 | 2.9s |
| 37 | claude-sonnet-4-6 | 1 | 579 | 87,402 | $0.0360 | 13.1s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
