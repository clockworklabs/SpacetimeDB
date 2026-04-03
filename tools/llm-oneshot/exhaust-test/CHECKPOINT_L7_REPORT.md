# Sequential Upgrade Benchmark — Checkpoint Report (L1–L7)

**Date:** 2026-04-03
**Variant:** sequential-upgrade
**Rules:** standard
**Model:** claude-sonnet-4-6

---

## Overview

Both backends were generated from scratch at L1, then incrementally upgraded through L7. Each level was manually graded after generation and after any fixes. Bugs were fixed via the fix prompt before proceeding to the next level.

---

## Feature Set by Level

| Level | New Feature Added | Cumulative Features |
|-------|-------------------|---------------------|
| L1 | Basic Chat, Typing Indicators, Read Receipts, Unread Counts | 4 |
| L2 | Scheduled Messages | 5 |
| L3 | Ephemeral/Disappearing Messages | 6 |
| L4 | Message Reactions | 7 |
| L5 | Message Editing with History | 8 |
| L6 | Real-Time Permissions (admin/kick/promote) | 9 |
| L7 | Rich User Presence (status, last active, auto-away) | 10 |

---

## Bug Log

### SpacetimeDB

| Level | Bug | Fix Attempts | Resolved |
|-------|-----|-------------|---------|
| L1 | Read receipts showed sender's own name | 1 | ✅ |
| L5 | No "Edit" button on hover over own messages | 1 | ✅ |
| L5 | Edit history panel not updating in real-time | 1 | ✅ |
| L6 | False "kicked" notification on room join | 1 | ✅ |

### PostgreSQL

| Level | Bug | Fix Attempts | Resolved |
|-------|-----|-------------|---------|
| L1 | Read receipts showed sender's own name | 1 | ✅ |
| L1 | No unread message count badges | 1 | ✅ |
| L1 | No "Leave room" button | 1 | ✅ |
| L5 | Edit history panel not updating in real-time | 1 | ✅ |
| L6 | No "Kick" or "Promote" buttons in member list | 1 | ✅ |
| L6 | Room member list not updating in real-time | 3 | ✅ |
| L7 | Users default to "invisible" instead of "online" on connect | 2 | ✅ |

---

## Cost & Time Metrics

### SpacetimeDB

| Phase | Cost | Duration | API Calls |
|-------|------|----------|-----------|
| L1 Generate | $1.37 | 6m 6s | 45 |
| L1 Fix | $0.17 | 41s | 7 |
| L2 Upgrade | $1.18 | 5m 40s | 29 |
| L3 Upgrade | $1.29 | 5m 39s | 32 |
| L4 Upgrade | $0.95 | 2m 56s | 32 |
| L5 Upgrade | $0.06 | 19s | 2 |
| L5 Fix | $0.84 | 3m 47s | 25 |
| L6 Upgrade | $1.16 | 5m 0s | 32 |
| L6 Fix | $0.19 | 57s | 10 |
| L7 Upgrade | $0.80 | 2m 52s | 27 |
| **Total** | **$8.01** | **33m 57s** | **241** |

### PostgreSQL

| Phase | Cost | Duration | API Calls |
|-------|------|----------|-----------|
| L1 Generate | $1.03 | 5m 0s | 34 |
| L1 Fix | $0.62 | 2m 56s | 23 |
| L2 Upgrade | $0.50 | 3m 16s | 19 |
| L3 Upgrade | $0.66 | 2m 30s | 26 |
| L4 Upgrade | $0.60 | 2m 0s | 23 |
| L5 Upgrade | $0.64 | 2m 18s | 22 |
| L5 Fix | $0.26 | 2m 0s | 12 |
| L6 Upgrade | $0.96 | 3m 16s | 31 |
| L6 Fix (3 attempts) | $0.87 | 4m 58s | 33 |
| L7 Upgrade | $0.90 | 2m 52s | 32 |
| L7 Fix (2 attempts) | $0.99 | 7m 56s | 36 |
| **Total** | **$8.03** | **38m 52s** | **291** |

### Combined

| Metric | SpacetimeDB | PostgreSQL | Combined |
|--------|------------|------------|---------|
| Total Cost | $8.01 | $8.03 | **$16.04** |
| Total Time | 33m 57s | 38m 52s | **72m 49s** |
| Total API Calls | 241 | 291 | **532** |
| Bugs Found | 4 | 7 | **11** |
| Fix Iterations | 4 | 11 | **15** |

---

## Observations

- **Cost parity by L7:** PostgreSQL started ~25% cheaper at L1–L4 but accumulated more bugs requiring multiple fix attempts, closing the gap by L7.
- **SpacetimeDB bugs were simpler:** 4 bugs, each fixed in 1 attempt. PostgreSQL had 7 bugs, with 3 requiring multiple fix attempts.
- **Real-time state management** is where PostgreSQL consistently struggled (member lists, status initialization) — expected given SpacetimeDB's built-in subscription model vs. PostgreSQL's manual WebSocket/polling approach.
- **L5 upgrade anomaly:** SpacetimeDB's L5 upgrade cost only $0.06 (19s) — Claude likely found the work mostly cached from prior session context. The subsequent fix was more expensive.
- **Auto-away timeout:** SpacetimeDB set 5-minute inactivity threshold (confirmed in code). Not tested manually due to impracticality.

---

## Quality Summary (L7, 10 features)

Both backends passed all 10 features after fixes. No features required abandonment or were deemed untestable (except auto-away timer, skipped as impractical).

| Feature | SpacetimeDB | PostgreSQL |
|---------|------------|------------|
| Basic Chat | ✅ | ✅ |
| Typing Indicators | ✅ | ✅ |
| Read Receipts | ✅ (1 fix) | ✅ (1 fix) |
| Unread Counts | ✅ | ✅ (1 fix) |
| Scheduled Messages | ✅ | ✅ |
| Ephemeral Messages | ✅ | ✅ |
| Message Reactions | ✅ | ✅ |
| Message Editing | ✅ (1 fix) | ✅ (1 fix) |
| Real-Time Permissions | ✅ (1 fix) | ✅ (2 fixes) |
| Rich User Presence | ✅ | ✅ (2 fixes) |
