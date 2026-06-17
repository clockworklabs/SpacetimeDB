# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 1
**Grading Method:** Manual browser interaction
**Setup:** fresh 20260617 baseline — cleaned prompts + official skills (typescript-server + typescript-client) + SpacetimeDB 2.6.0

---

## Feature 1: Basic Chat (Score: 3 / 3)
- [x] Set a display name
- [x] Create chat rooms
- [x] Join/leave rooms
- [x] Send messages to joined rooms
- [x] Online users displayed
- [x] Basic validation

## Feature 2: Typing Indicators (Score: 3 / 3)
- [x] Typing broadcast to other room members
- [x] Auto-expires after inactivity
- [x] "X is typing…" UI

## Feature 3: Read Receipts (Score: 3 / 3)
- [x] Tracks who has seen which messages
- [x] "Seen by X" under messages
- [x] Real-time updates

## Feature 4: Unread Message Counts (Score: 3 / 3)
- [x] Unread badge on room list
- [x] Tracks last-read per user per room
- [x] Real-time updates (arrive + clear)

**Browser Test Observations:** Everything works on first generate — all four features behave
correctly in real time with no refresh, no console errors. Validates the official-skill +
2.6.0 swap end-to-end (publish, bindings, build, deploy, and correct runtime behavior).

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1. Basic Chat | 3/3 | |
| 2. Typing Indicators | 3/3 | |
| 3. Read Receipts | 3/3 | |
| 4. Unread Counts | 3/3 | |
| **TOTAL** | **12/12** | |

**Reprompt count:** 0 (passed on first generate)
**Cost:** L1 generate $2.15
**Note:** ~2.6x the published STDB L1 generate ($0.84). Single data point — driven by ~2.9x
output / ~3x turns (thrash signature). Full 12-level run will show whether this is systematic
(skill/2.6.0) or variance. Fresh baseline; not comparable to the published STDB run.
