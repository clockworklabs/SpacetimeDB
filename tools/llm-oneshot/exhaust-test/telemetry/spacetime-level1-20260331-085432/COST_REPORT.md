# Cost Report

**App:** chat-app
**Backend:** spacetime
**Level:** 1
**Date:** 2026-04-01
**Started:** 2026-03-31T08:54:32-0400

## Summary

| Metric                  | Value |
|-------------------------|-------|
| Total input tokens      | 36 |
| Total output tokens     | 21,479 |
| Total tokens            | 21,515 |
| Cache read tokens       | 1,854,331 |
| Cache creation tokens   | 38,623 |
| Total cost (USD)        | $1.7057 |
| Total API time          | 354.5s |
| API calls               | 34 |

## Per-Call Breakdown

| # | Model | Input | Output | Cache Read | Cost | Duration |
|---|-------|-------|--------|------------|------|----------|
| 1 | claude-opus-4-6 | 1 | 401 | 27,639 | $0.0324 | 8.8s |
| 2 | claude-opus-4-6 | 1 | 862 | 29,003 | $0.0398 | 15.2s |
| 3 | claude-opus-4-6 | 1 | 2,924 | 29,604 | $0.0938 | 55.1s |
| 4 | claude-opus-4-6 | 1 | 2,415 | 30,550 | $0.0942 | 27.6s |
| 5 | claude-opus-4-6 | 1 | 167 | 33,520 | $0.0366 | 6.9s |
| 6 | claude-opus-4-6 | 1 | 235 | 36,546 | $0.0362 | 6.5s |
| 7 | claude-opus-4-6 | 1 | 174 | 38,476 | $0.0475 | 5.6s |
| 8 | claude-opus-4-6 | 1 | 1,253 | 42,296 | $0.0563 | 18.6s |
| 9 | claude-opus-4-6 | 1 | 207 | 42,903 | $0.0380 | 6.4s |
| 10 | claude-opus-4-6 | 1 | 4,999 | 44,718 | $0.1491 | 57.5s |
| 11 | claude-opus-4-6 | 1 | 3,060 | 45,004 | $0.1307 | 32.1s |
| 12 | claude-opus-4-6 | 1 | 162 | 50,082 | $0.0487 | 3.9s |
| 13 | claude-opus-4-6 | 1 | 161 | 53,221 | $0.0320 | 3.5s |
| 14 | claude-opus-4-6 | 1 | 370 | 54,960 | $0.0745 | 7.9s |
| 15 | claude-opus-4-6 | 1 | 350 | 60,999 | $0.0428 | 4.6s |
| 16 | claude-opus-4-6 | 1 | 339 | 61,572 | $0.0421 | 5.5s |
| 17 | claude-opus-4-6 | 1 | 278 | 62,021 | $0.0406 | 4.5s |
| 18 | claude-opus-4-6 | 1 | 315 | 62,440 | $0.0413 | 7.1s |
| 19 | claude-opus-4-6 | 1 | 411 | 62,798 | $0.0452 | 7.1s |
| 20 | claude-opus-4-6 | 1 | 246 | 63,359 | $0.0409 | 4.4s |
| 21 | claude-opus-4-6 | 1 | 172 | 63,850 | $0.0389 | 3.9s |
| 22 | claude-opus-4-6 | 1 | 162 | 64,280 | $0.0374 | 3.3s |
| 23 | claude-opus-4-6 | 1 | 104 | 64,470 | $0.0377 | 3.2s |
| 24 | claude-opus-4-6 | 1 | 158 | 64,931 | $0.0372 | 3.4s |
| 25 | claude-opus-4-6 | 1 | 161 | 65,055 | $0.0384 | 14.2s |
| 26 | claude-opus-4-6 | 1 | 101 | 65,348 | $0.0371 | 2.6s |
| 27 | claude-opus-4-6 | 1 | 105 | 65,648 | $0.0362 | 3.3s |
| 28 | claude-opus-4-6 | 1 | 156 | 65,775 | $0.0383 | 3.8s |
| 29 | claude-opus-4-6 | 1 | 223 | 66,009 | $0.0403 | 4.8s |
| 30 | claude-opus-4-6 | 1 | 158 | 66,284 | $0.0391 | 3.6s |
| 31 | claude-opus-4-6 | 1 | 106 | 66,284 | $0.0397 | 3.6s |
| 32 | claude-opus-4-6 | 3 | 175 | 67,801 | $0.0399 | 5.4s |
| 33 | claude-opus-4-6 | 1 | 106 | 68,058 | $0.0393 | 3.8s |
| 34 | claude-opus-4-6 | 1 | 263 | 68,827 | $0.0436 | 6.5s |

## Notes

- Token counts are exact values from Claude Code's OpenTelemetry instrumentation
- Cache read tokens represent prompt caching (repeated context sent at reduced cost)
- Total cost includes both input and output token pricing
