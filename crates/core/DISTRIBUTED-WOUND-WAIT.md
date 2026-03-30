# Distributed Wound-Wait for 2PC Deadlock Breaking

## Problem

Distributed reducers can deadlock when one distributed transaction holds a participant lock on one database and waits on another database locked by a younger distributed transaction. Existing 2PC ensures atomic commit/abort, but it does not resolve distributed lock cycles.

## Chosen Model

- Use wound-wait.
- Transaction identity is `GlobalTxId`.
- `GlobalTxId.creator_db` is the authoritative coordinator database.
- Participant and coordinator runtime state are keyed by `GlobalTxId`.
- `prepare_id` remains a participant-local 2PC phase handle only.
- Older transactions wound younger transactions.
- Younger transactions wait behind older transactions.
- Lock acquisition is managed by an owner-aware scheduler that tracks running and pending `GlobalTxId`s.
- A wound RPC is required because a younger lock holder may belong to a distributed transaction coordinated on a different database.

## Runtime Model

- Add a distributed session registry keyed by `GlobalTxId`.
- Session tracks role, state, local `prepare_id`, coordinator identity, participants, and a local wounded/abort signal.
- Add a per-database async lock scheduler for distributed reducer write acquisition.
- Scheduler state includes current running owner, pending queue/set, and wounded marker for the current owner.
- Requesters await scheduler admission before reducer execution starts a local mutable transaction.

## Wound Protocol

Participant detecting conflict compares requester and owner by `GlobalTxId` ordering.

- If requester is older:
  - mark local owner as wounded
  - send wound RPC to coordinator `GlobalTxId.creator_db` if needed
  - keep requester pending until local owner releases
- If requester is younger:
  - requester stays pending

Wound RPC is idempotent and targets the distributed session at the coordinator.

Coordinator receiving wound:

- transitions session to `Aborting`
- sets wounded flag
- aborts local execution cooperatively
- fans out abort to known prepared participants

## Safe Points

- Before remote reducer calls
- Before PREPARE / COMMIT path work
- After reducer body returns, before expensive post-processing

On safe-point wound detection:

- rollback local tx
- unregister scheduler ownership
- wake waiters
- surface retryable `wounded` error

## Compatibility

- Keep existing `/2pc/commit`, `/2pc/abort`, `/2pc/status`, and ack-commit flows.
- Add a new wound endpoint.
- `/prepare` must propagate `GlobalTxId` the same way `/call` already does.
- No durable format change unless recovery work later proves it necessary.

## Implementation Sequence

### 1. Propagate `GlobalTxId` through 2PC prepare path

- Update outgoing 2PC prepare requests to send `X-Spacetime-Tx-Id`.
- Update incoming `/prepare/:reducer` to parse `X-Spacetime-Tx-Id`.
- Thread `GlobalTxId` through `prepare_reducer` and any participant execution params.
- Ensure recovered/replayed participant work can recover or reconstruct the same session identity.

### 2. Replace minimal prepared registry with `GlobalTxId` session registry

- Extend the current prepared transaction registry into a session manager keyed by `GlobalTxId`.
- Track:
  - role
  - state
  - local `prepare_id`
  - participants
  - coordinator identity
  - wounded/abort signal
- Provide lookup by both `GlobalTxId` and `prepare_id`.

### 3. Add distributed lock scheduler

- Add an async scheduler in `core`, adjacent to reducer tx startup, not inside raw datastore locking.
- Track running owner and pending `GlobalTxId`s.
- Require distributed reducer write acquisition to await scheduler admission before blocking datastore acquisition.
- Implement wound-wait ordering and wakeup behavior there.

### 4. Add wound RPC endpoint and coordinator handler

- Add `POST /v1/database/:name_or_identity/2pc/wound/:global_tx_id`.
- Parse and route by `GlobalTxId.creator_db`.
- Coordinator session handler must:
  - mark session `Aborting`
  - set wounded flag
  - begin participant abort fanout
  - behave idempotently

### 5. Add cooperative abort checks in reducer execution

- Add wound checks at required safe points.
- On wound:
  - rollback
  - unregister running owner
  - notify scheduler waiters
  - surface retryable wounded error

### 6. Integrate scheduler + wound with 2PC transitions

- Ensure PREPARE, COMMIT, ABORT, and recovery all keep scheduler and session registry consistent.
- Make local owner release happen on all terminal paths.
- Keep participant recovery compatible with session state.

### 7. Add tests

- Scheduler ordering and wakeup tests
- Local same-database wound tests
- Distributed deadlock cycle tests
- Wound RPC idempotency tests
- Recovery tests for wounded prepared transactions
- Regression tests for existing 2PC success/failure flows

## Acceptance Criteria

- Distributed deadlock cycles are broken deterministically by wound-wait.
- Older distributed transactions eventually proceed without manual intervention.
- Younger distributed transactions abort globally, not just locally.
- `/prepare` and `/call` both carry `GlobalTxId`.
- Existing 2PC happy paths continue to pass.
- Repeated wound or abort requests are safe and idempotent.

## Assumptions

- `GlobalTxId.creator_db` is always the coordinator database.
- `GlobalTxId` ordering is the authoritative age/tie-break rule.
- Cooperative abort at safe points is sufficient for v1; no preemptive interruption is required.
- Lock scheduler state is in-memory runtime state, not durable state.
