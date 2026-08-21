<!--
  Shared, backend-agnostic body of the team-chat task instruction.
  Each backend's instruction.md = this body + a backend-specific "Contract"
  section pinning the exact identifiers the grader connects to. This file is
  the single source of truth for the app spec.
-->

# Task: team chat backend

Build the backend for a team chat application (think a minimal Slack/Discord
server): named rooms with owners and members, real-time messaging with
editing and deletion, per-member unread counters, user presence, and a credit
system where users can tip each other.

The application is graded **behaviorally** by an automated verifier that
drives the running backend through its real client SDK with multiple
concurrent clients. It checks functional correctness, transactional
correctness under concurrency, real-time delivery, durability across a
backend restart, and basic performance. Partial credit is awarded per check.
Grading does not inspect your source code — only observable behavior counts.

## Data model (required behavior, not storage advice)

- **User** — `username` (unique string), `status` (one of `"online"`,
  `"away"`, `"offline"`), `balance` (integer credits).
- **Room** — `name` (unique string), `owner` (a username).
- **Membership** — which users are in which rooms, plus per-member
  `last_read_seq` and `unread` counters (see Unread rules).
- **Message** — belongs to a room; has a server-assigned per-room sequence
  number `seq`, a client-supplied id `client_msg_id`, `sender`, `text`,
  `edited` flag, `deleted` flag.

## Operations

All operations validate their inputs and **fail with an error** when a rule
below is violated. "Fails" must be observable to the caller through the
backend's normal error mechanism (rejected mutation / failed reducer call).

1. **register(username)** — Creates the user with `status = "online"` and
   `balance = 100`. If the user already exists, the call **succeeds** and
   leaves the existing user completely unchanged (idempotent; must NOT reset
   balance or status).
2. **set_status(username, status)** — Sets presence. Fails for unknown users
   or a status outside the three allowed values.
3. **create_room(username, room)** — Creates a room owned by `username`, who
   automatically becomes a member (with `last_read_seq = 0`, `unread = 0`).
   Fails for unknown users or if the room name already exists.
4. **join_room(username, room)** — Adds the user as a member with
   `last_read_seq = 0` and `unread` equal to the number of messages already
   in the room sent by other users. Fails for unknown user/room. If already
   a member, succeeds without changing the existing membership state.
5. **leave_room(username, room)** — Removes the membership. Fails for
   unknown user/room, if not a member, or if the user is the room's owner
   (owners cannot leave).
6. **kick(actor, room, target)** — Removes `target`'s membership. Fails
   unless `actor` is the room's owner; fails if `target` is the owner or not
   a member.
7. **send_message(sender, room, text, client_msg_id)** — Appends a message.
   Fails for: unknown user/room, sender not a member, empty `text`, `text`
   longer than 4000 characters. **Idempotency:** if a message with the same
   `client_msg_id` already exists in the room, the call **succeeds** without
   creating a second message (safe retry). Otherwise the server assigns
   `seq` (see Sequence rules), stores the message with `edited = false`,
   `deleted = false`, and **atomically** increments `unread` by 1 for every
   member of the room except the sender.
8. **edit_message(actor, room, client_msg_id, new_text)** — Replaces the
   text and sets `edited = true`. Fails unless `actor` is the original
   sender; fails if the message is deleted or `new_text` fails the same
   validation as send.
9. **delete_message(actor, room, client_msg_id)** — Tombstones the message:
   sets `deleted = true` and **clears `text` to the empty string**. Allowed
   for the original sender or the room owner; fails for anyone else or if
   already deleted. The original text must not be recoverable by any client
   afterwards (late-joining clients must see the tombstone, never the text).
10. **mark_read(user, room, up_to_seq)** — Sets the member's
    `last_read_seq = max(current, up_to_seq)` (it never decreases), then
    recomputes `unread` per the Unread rules. Fails if not a member.
11. **tip(from_user, to_user, amount)** — Atomically transfers `amount`
    credits. Fails for: unknown users, `from_user == to_user`,
    `amount <= 0`, or `from_user`'s balance below `amount`. Balances must
    never go negative and total credits must be conserved, including under
    concurrent tips.

## Sequence rules (transactional correctness)

- `seq` is **per room**, assigned by the server, starting at 1 for the
  room's first message and increasing by exactly 1 per stored message —
  **no gaps, no duplicates**, even when many clients send concurrently.
- The counter must survive a backend restart: the first message stored
  after a restart continues from the pre-restart maximum (durable counter —
  an in-memory counter that resets and reuses sequence numbers fails).

## Unread rules

At any quiescent moment, for every member `u` of room `r`:
`unread(u, r) == count of messages m in r with m.seq > last_read_seq(u, r) and m.sender != u`.
Deleted (tombstoned) messages still count — they were sent. This invariant
must hold exactly, including after concurrent sends from multiple clients
(the per-message unread increments must be atomic with message insertion).

## Real-time requirements

Connected clients observe changes via **push** (the backend's real-time
subscription mechanism, not client polling):

- A client subscribed to a room receives every existing message at
  subscribe time (in `seq` order, reflecting current edited/deleted state)
  and every subsequent message thereafter, exactly once, in `seq` order.
- Message **edits and deletions** are pushed to subscribers in real time.
- Membership changes (join / leave / kick / read-state updates) are pushed
  to subscribers of the room's membership.
- User changes (status, balance) are pushed to subscribers of the user list.
- Subscriptions are **per room**: a client subscribed to room A must not
  receive room B's messages.

## Durability requirements

All state — users, balances, rooms, memberships, read state, messages
(including edits and tombstones), the idempotency record of seen
`client_msg_id`s, and the per-room sequence counters — must survive a
backend process restart. The verifier will restart the backend mid-run,
reconnect fresh clients, and check every piece of state plus continued
real-time operation.

## Performance requirements

Modest but real: with a handful of clients, message delivery latency
(send-call to another subscribed client receiving the push) should be well
under 1.5 seconds at p95, and a burst of 120 messages from 3 concurrent
senders should be fully delivered to a subscriber within 45 seconds. A
correct implementation on this backend passes these comfortably; polling
loops with long intervals do not.

## What is graded

The verifier awards partial credit across four weighted groups:
correctness & transactions (0.40), real-time behavior (0.30), durability
across restart (0.20), performance (0.10). Every check is machine-verified
through the client SDK against the running backend.

Your implementation MUST expose exactly the identifiers named in the
**Contract** section below so the grader can connect.

## Contract (Convex)

A self-hosted Convex backend is already running in this container at
`http://127.0.0.1:3210`. Build a Convex app and deploy it with:

    export CONVEX_SELF_HOSTED_URL=http://127.0.0.1:3210
    export CONVEX_SELF_HOSTED_ADMIN_KEY='convex-stack-bench|01be067fd1488e360c17a915fd342953e6450766d4831138261df4371e27342009f370391e'
    npx convex deploy -y

The grader connects with the official Convex client SDK and calls your
functions **by these exact names** (module.function) with these argument
object keys. You may design your table schema freely, but every function
below must exist with these signatures and behaviors:

Mutations (throw an `Error` to report failures):

    users.register({ username })
    users.setStatus({ username, status })
    rooms.createRoom({ username, room })
    rooms.joinRoom({ username, room })
    rooms.leaveRoom({ username, room })
    rooms.kick({ actor, room, target })
    messages.send({ sender, room, text, clientMsgId })
    messages.edit({ actor, room, clientMsgId, newText })
    messages.remove({ actor, room, clientMsgId })
    messages.markRead({ user, room, upToSeq })
    credits.tip({ fromUser, toUser, amount })

Queries (all reactive — the grader subscribes to them via the SDK's onUpdate
and expects pushed updates on every relevant change):

    users.get({ username })      -> { username, status, balance } | null
    users.list({})               -> [{ username, status, balance }]
    rooms.getOwner({ room })     -> string | null
    rooms.listMembers({ room })  -> [{ user, lastReadSeq, unread }] sorted by user
    messages.list({ room })      -> [{ seq, clientMsgId, sender, text, edited, deleted }]
                                    sorted by seq ascending

Notes:

- Convex mutations are serializable transactions; use that to keep the
  per-room `seq` counter gapless and the unread increments atomic with the
  message insert, including under concurrent calls.
- The backend stores state in SQLite under /convex/data; the verifier
  restarts the backend process and expects all state (including per-room seq
  counters and clientMsgId dedupe behavior) to survive.
- Do not modify or stop the running backend; deploy your app to it.
