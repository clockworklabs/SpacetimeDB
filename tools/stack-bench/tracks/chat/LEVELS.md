# Chat track — level sequence

The chat track is retained as a collaboration workload built around rooms,
messages, presence, and per-user state. Its definitions remain supported, but
its current reference fixtures are blocked and must be rebuilt before chat is
presented as a newly qualified cross-stack comparison.

Levels are cumulative and ordered by the property they make measurable.

## Status

| Level | Product scope | Status |
|---|---|---|
| L1 | accounts, rooms, messages, presence, typing, and read state | definitions available; reference rebuild required |
| L2 | private rooms, membership, invitations, removal, profiles, and friends | definitions available; reference rebuild required |
| L3 | contended counters, capacity, and transfers | planned |
| L4 | deferred and expiring work | planned |
| L5 | correctness and efficiency under load | planned |

`track.json` declares the available suites and the current validation boundary.
The reference registry records why each chat reference is blocked. A blocked
reference cannot be used as qualification evidence.

## L1 — Basic chat with accounts

L1 establishes stable identity, durable shared state, ephemeral room-scoped
state, and per-user derived state:

- account creation and sessions;
- room creation and messaging;
- presence and typing indicators;
- read receipts and unread counts;
- delivery, ordering, reconnect, and system invariants declared by the track.

Each browser actor runs in an isolated context, so identity and per-user state
are measured independently.

## L2 — Authorization and people

L2 adds resource ownership, private rooms, membership, invitations, removal,
profiles, friend requests, and presence. Its checks cover both visible behavior
and direct authorization or privacy evidence where the transport can be
exercised conclusively.

## L3 — Contended state

The L3 target introduces reaction or vote counters, bounded capacity, unique
claims, and balance transfers. Exact arithmetic and winner counts are the
intended observable outcomes. No current L3 release is qualified.

## L4 — Deferred and expiring work

The L4 target covers scheduled messages, expiry, and reminders across process
restart. No current L4 release is qualified.

## L5 — Volume

The L5 target measures correctness, propagation latency, throughput, and query
growth with many rooms, a large message history, and concurrent clients. No
current L5 workload is qualified.

## Running and extending the track

Use the package entrypoints documented in the primary `README.md`: `bench` runs
the build/grade/correction loop, `run-suite.mjs` grades a prepared app, and
`report-bugs.mjs` produces structured correction input. New scored checks require
reference, null-control, and defect-detection evidence before promotion.
