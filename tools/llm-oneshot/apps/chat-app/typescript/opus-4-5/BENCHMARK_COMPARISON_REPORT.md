# Chat App Benchmark Comparison Report
## PostgreSQL vs SpacetimeDB

**Report Generated:** 2026-01-05  
**AI Model:** Claude Opus 4.5  
**Prompt Level:** 9 (Private Rooms and DMs) — Features 1-12

---

## Executive Summary

| Platform | Runs | Avg Score | Best Score | Worst Score |
|----------|------|-----------|------------|-------------|
| **SpacetimeDB** | 4 | **34.25 / 36 (95.1%)** | 36/36 (100%) | 32.5/36 (90.3%) |
| **PostgreSQL** | 3 | **25.92 / 36 (72.0%)** | 27.5/36 (76.4%) | 23.0/36 (63.9%) |

**Key Finding:** SpacetimeDB implementations consistently outperformed PostgreSQL by **~23 percentage points** on average, with significantly fewer real-time synchronization bugs.

---

## Individual Run Results

### SpacetimeDB Implementations

| Timestamp | Score | % | LOC Backend | LOC Frontend | Files |
|-----------|-------|---|-------------|--------------|-------|
| 2026-01-02 16:29:18 | 34/36 | 94.4% | 1,008 | ~1,500 | 26 |
| 2026-01-02 17:05:00 | 32.5/36 | 90.3% | 879 | 1,803 | 16 |
| 2026-01-02 17:13:17 | 34.5/36 | 95.8% | ~1,500 | ~1,700 | 12 |
| 2026-01-05 18:00:00 | **36/36** | **100%** | ~650 | ~750 | 11 |

### PostgreSQL Implementations

| Timestamp | Score | % | LOC Backend | LOC Frontend | Files |
|-----------|-------|---|-------------|--------------|-------|
| 2026-01-04 12:00:00 | 27.5/36 | 76.4% | 1,689 | 2,849 | 23 |
| 2026-01-04 16:00:00 | 27.25/36 | 75.7% | 1,004 | 2,285 | 21 |
| 2026-01-04 18:00:00 | 23.0/36 | 63.9% | 1,131 | 2,222 | 20 |

---

## Feature-by-Feature Comparison

| Feature | Max | SpacetimeDB Avg | PostgreSQL Avg | Δ | Winner |
|---------|-----|-----------------|----------------|---|--------|
| 1. Basic Chat | 3 | **3.0** | 2.0 | +1.0 | 🟢 STDB |
| 2. Typing Indicators | 3 | **3.0** | **3.0** | 0 | 🟡 Tie |
| 3. Read Receipts | 3 | **3.0** | 1.83 | +1.17 | 🟢 STDB |
| 4. Unread Counts | 3 | **3.0** | 0.83 | +2.17 | 🟢 STDB |
| 5. Scheduled Messages | 3 | **2.5** | 2.17 | +0.33 | 🟢 STDB |
| 6. Ephemeral Messages | 3 | **3.0** | **3.0** | 0 | 🟡 Tie |
| 7. Message Reactions | 3 | **3.0** | 2.0 | +1.0 | 🟢 STDB |
| 8. Message Editing | 3 | **3.0** | 2.67 | +0.33 | 🟢 STDB |
| 9. Real-Time Permissions | 3 | 2.25 | 1.58 | +0.67 | 🟢 STDB |
| 10. Rich Presence | 3 | **2.88** | 2.67 | +0.21 | 🟢 STDB |
| 11. Message Threading | 3 | **3.0** | 1.5 | +1.5 | 🟢 STDB |
| 12. Private Rooms & DMs | 3 | **3.0** | 2.17 | +0.83 | 🟢 STDB |

**SpacetimeDB wins 10 features, ties 2, loses 0.**

---

## Real-Time Synchronization Analysis

### Areas Where SpacetimeDB Excelled

| Feature | SpacetimeDB | PostgreSQL | Observation |
|---------|-------------|------------|-------------|
| **Unread Counts** | Works perfectly | Inconsistent, doesn't clear on room entry | STDB's reactive subscriptions handle state sync automatically |
| **Read Receipts** | Real-time sync | Often requires page refresh | Socket.io event handling incomplete in Postgres |
| **Message Threading** | Full real-time updates | Replies don't sync, thread view stale | STDB subscription model keeps thread views live |
| **Private Room Invites** | Accept/decline works | Users can accept but can't access room | PostgreSQL event flow had missing socket joins |

### Common PostgreSQL Issues (Across All Runs)

1. **Room Duplication Bug** — Created rooms appear twice in list (3/3 runs)
2. **Kicked Users Still Connected** — Socket not disconnected on kick (3/3 runs)
3. **Thread View Not Real-Time** — Must close and reopen to see new replies (3/3 runs)
4. **Unread Counts Inconsistent** — Badge counts unreliable (3/3 runs)
5. **Edit History Modal Stale** — Doesn't refresh when edits made while open (3/3 runs)
6. **Invite System Broken** — Invites accepted but room not accessible (2/3 runs)

### Common SpacetimeDB Issues (Across All Runs)

1. **Kicked User UI Delay** — UI doesn't immediately update on kick (2/4 runs)
2. **Scheduled Messages Panel Visibility** — Panel only shows when messages exist (2/4 runs)
3. **Admin Tools for Public Rooms** — UI limited to private rooms in some implementations (1/4 runs)
4. **Auto-Away Not Implemented** — Missing in some runs (1/4 runs)

---

## Code Complexity Comparison

### Lines of Code

| Metric | SpacetimeDB Avg | PostgreSQL Avg | Δ |
|--------|-----------------|----------------|---|
| Backend LOC | **1,009** | 1,275 | -21% |
| Frontend LOC | **1,438** | 2,452 | -41% |
| **Total LOC** | **2,447** | **3,727** | **-34%** |

SpacetimeDB implementations required **~34% less code** while achieving **~32% higher scores**.

### External Dependencies

| SpacetimeDB | PostgreSQL |
|-------------|------------|
| spacetimedb | drizzle-orm |
| react | postgres |
| vite | express |
| | socket.io |
| | jsonwebtoken |
| | node-cron |
| | cors |
| | socket.io-client |

SpacetimeDB: **3 dependencies** vs PostgreSQL: **8+ dependencies**

## Bug Pattern Analysis

### PostgreSQL Bug Categories

| Category | Occurrences | Impact |
|----------|-------------|--------|
| Socket.io room/event sync | 12 | High — users miss updates |
| State not propagated to all clients | 8 | High — inconsistent views |
| UI/server state mismatch | 6 | Medium — requires refresh |
| Authorization gaps | 4 | Medium — security concerns |
| Race conditions | 3 | Low — edge cases |

### SpacetimeDB Bug Categories

| Category | Occurrences | Impact |
|----------|-------------|--------|
| UI conditional logic | 4 | Low — features hidden but work |
| Missing optional features | 3 | Low — auto-away, ban vs kick |
| SDK quirks | 1 | Low — `t.product()` workaround |

---

## Scoring Distribution

### Score Ranges

| Range | SpacetimeDB | PostgreSQL |
|-------|-------------|------------|
| 95-100% | 2 (50%) | 0 |
| 90-94% | 2 (50%) | 0 |
| 75-89% | 0 | 2 (67%) |
| 60-74% | 0 | 1 (33%) |
| < 60% | 0 | 0 |

### Standard Deviation

- **SpacetimeDB:** σ = 1.5 (very consistent)
- **PostgreSQL:** σ = 2.3 (more variation)

---

## Raw Data Summary

### All Scores

```
SpacetimeDB: [34, 32.5, 34.5, 36] → Mean: 34.25, Median: 34.25
PostgreSQL:  [27.5, 27.25, 23]   → Mean: 25.92, Median: 27.25
```

## Conclusion

SpacetimeDB implementations achieved a **95.1% average score** compared to PostgreSQL's **72.0%**, with significantly less code and fewer real-time synchronization bugs. The primary driver is SpacetimeDB's reactive subscription model, which eliminates the manual event-wiring that causes most PostgreSQL implementation failures.

For AI code generation benchmarks, SpacetimeDB's declarative approach produces more reliable real-time applications "out of the box," while PostgreSQL implementations consistently require manual debugging of Socket.io event flows.
