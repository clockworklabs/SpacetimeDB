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
