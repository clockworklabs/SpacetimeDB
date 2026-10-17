# Chat progression

The chat track covers rooms, messages, presence, and per-user state. Its L1 and
L2 definitions are available, but it has no active reference fixtures. Do not
present chat results as a qualified cross-stack comparison.

`track.json` is the source of truth for available suites and the validation
boundary.

## L1: Chat and accounts

- account creation and sessions;
- room creation and messages;
- presence and typing state;
- read receipts and unread counts;
- ordering, reconnect, and system invariants.

Each actor uses a separate browser context so identity and per-user state are
measured independently.

## L2: Authorization and people

- room ownership and private rooms;
- membership, invitations, and removal;
- profiles and friend requests;
- presence privacy and authorization.

## Planned work

- L3 adds contended counters, bounded capacity, unique claims, and transfers.
- L4 adds scheduled and expiring work.
- L5 adds correctness and efficiency under load.

Planned work is not launchable or qualified. New scored checks require matching
reference, null-control, and defect-detection evidence before promotion.

## Current measurement limits

All chat definitions remain unqualified (`validatedThrough: 0`), with no active
reference fixtures. `01-contention-wip.json` is not selected by `track.json`.
The systems suites are selected but their criteria carry zero points.

Raw authorization replays depend on captured HTTP requests. The current chat
scenarios have no named reducer fallback. An unavailable replay is inconclusive;
a hidden control or absent DOM row does not prove server authorization. DOM-only
privacy criteria measure display isolation, not absence of data on the wire.

System purge probes execute the application's own back-office script. They
observe open and fresh clients but do not independently read database rows.
Concurrent client actions do not prove server overlap. A passing burst does not
establish a failure rate or sustained capacity. The inactive contention probes
need cross-stack reference and targeted-defect evidence before promotion.

Replay completion is checked before crediting unchanged friendship state, but
friendship rows are still observed through the interface, not a trusted stored
relationship read. Receipt stability samples a bounded interval; it does not
prove that the display never flickers. Preserve these limits in any report.

The rate-limit rollback criterion does not establish the server-arrival interval
of its two UI sends or acknowledge the first delivery. Waiting for delivery
before the second send could exceed the allowed interval on a correct app.
This unresolved precondition blocks qualification of that criterion; changing
it into a different validation test would not qualify server rate limiting.
