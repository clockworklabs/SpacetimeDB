# Distributed Wound-Wait for Two-Phase Commit

This document explains the prototype on `jsdt/2pc-fake-durability` at
`38f335d0ea` (April 23, 2026), which builds on `jsdt/2pc-wound-wait`.
It describes that revision, not a guarantee about other branches or production
behavior. It consolidates the original implementation plan with an explanation
of the protocol and later refinements.

## Why 2PC needs deadlock prevention

Two-phase commit (2PC) coordinates a single commit or abort decision across
databases. In the prepare phase, participants make their work ready to commit;
in the decision phase, the coordinator tells them to commit or abort. A prepared
participant must retain the resources needed to honor that decision.

Atomic agreement does not determine which competing transaction gets a lock.
For example, transaction A can hold database X while requesting Y, while B
holds Y and requests X. Neither can finish preparing, so neither reaches the
decision that would release its resources.

Wound-wait breaks this cycle by assigning transactions a consistent priority.
A **wound** requests that a competing transaction abort its entire distributed
attempt, releasing its resources so another transaction can proceed.

## The basic rule

Smaller transaction IDs have higher priority, described here as “older.”

| Requester versus lock owner | Wound-wait action |
| --- | --- |
| Requester is older | Wound the younger owner, then wait for it to release ownership. |
| Requester is younger | Wait for the older owner to finish. |

Wounding does not immediately transfer a lock. The victim must stop execution
and clean up safely first. Under the classical rule, lasting wait dependencies
point from younger to older transactions, so they cannot form an age-ordered
cycle. Temporary waits during abort and the prototype's grace period are still
possible. Progress depends on cancellation and decision delivery completing.

## Transaction identity and retries

`GlobalTxId` is ordered lexicographically by `start_ts`, `creator_db`, `nonce`,
and `attempt`. The timestamp establishes priority; the remaining fields break
ties and identify attempts. “Older” means this ordering, not a measurement of
the true elapsed age across machines.

`creator_db` identifies the coordinator. Calls propagate the global identity
using `X-Spacetime-Tx-Id`, so all databases compare the same transaction IDs.
The separate `prepare_id` identifies a participant's prepared work for 2PC
completion; it does not determine wound-wait priority.

`next_attempt()` increments only `attempt`, preserving the original timestamp,
creator, and nonce. The client connection retry path uses this when retrying a
wounded attempt. Preserving priority prevents each retry from becoming a newly
young transaction, although this alone is not a proof of starvation freedom.

## Walking through a conflict

Suppose A is older than B:

1. A holds X and B holds Y.
2. B requests X. Because A is older, B waits.
3. A requests Y. It discovers the younger owner B and triggers the wound flow
   if B still owns Y after the applicable grace period.
4. B's coordinator is notified that B must abort. Running work observes the
   cancellation, and prepared work is resolved through the 2PC decision path.
5. B's work rolls back and releases ownership. A can acquire Y and finish.
6. B can retry as a new attempt after cleanup.

Aborting only B's work on Y would be insufficient: B might have work prepared
elsewhere. The wound concerns the global attempt, not just the conflicting lock.

## How the prototype schedules work

`GlobalTxManager` tracks sessions and admission to a per-database execution
slot before mutable transaction acquisition. This is a database-level scheduler,
not a row-lock wound-wait implementation. Sessions track coordinator or
participant role, state, prepared handles, participants, and a wound signal.
The scheduler and session registry are in-memory runtime state; recovery must
reconstruct the information needed to resolve prepared work.

The implementation refines the basic rule:

- A requester coordinated on this database does not initiate wounds while
  waiting for admission. A remote requester can wound a younger owner.
- Remote waiters are selected before local waiters; each group uses ordered
  waiter keys. Admission is therefore not one global oldest-first queue.
- A remote younger owner gets a configurable grace period to finish naturally.
  The manager's default is 30 milliseconds. An owner coordinated locally gets
  zero grace when an eligible remote requester wounds it.
- After waiting, the scheduler checks that the same owner still holds the slot.
  If it has finished, there is no need to wound it.
- Wounded waiters cancel their admission attempt. Ownership release and waiter
  cleanup notify the next eligible waiter.

These details reduce unnecessary aborts and distinguish local admission from
cross-database contention; they should not be mistaken for the unqualified
classical rule applied to every lock request.

## Inside `GlobalTxManager`

Each database has its own manager. “Global” refers to the transaction identities
it tracks: the manager combines a local registry of distributed transaction
sessions with a scheduler for this database's execution slot.

```rust
pub struct GlobalTxManager {
    local_database_identity: Identity,
    sessions: Mutex<HashMap<GlobalTxId, Arc<GlobalTxSession>>>,
    prepare_to_tx: Mutex<HashMap<String, GlobalTxId>>,
    lock_state: Mutex<LockState>,
    wound_grace_period: Duration,
}
```

### Sessions and prepared transaction lookup

`sessions` answers “what do we know about this global transaction here?”
`ensure_session()` creates an entry if absent and otherwise returns the existing
session. Each `GlobalTxSession` contains:

- The transaction ID, its coordinator identity, and its role on this database
  (`Coordinator` or `Participant`).
- A mutex-protected state: `Running`, `Preparing`, `Prepared`, `Aborting`,
  `Aborted`, `Committing`, or `Committed`.
- An optional local `prepare_id` and a list of participant database identities
  paired with their prepare IDs. The list can retain multiple prepared handles
  for the same database.
- An atomic wounded flag for synchronous checks and a `watch` channel used to
  notify asynchronous listeners when the session is wounded.

The `Arc` lets execution and cancellation code share a session after releasing
the registry mutex. Mutable session fields have their own synchronization.

`prepare_to_tx` is a reverse index from a local prepared handle to its global
transaction. It connects commit/abort requests expressed in terms of a
`prepare_id` to the session registry. Setting or removing a mapping also updates
the session's local prepare ID when that session exists.

### Ownership and waiter indexes

`lock_state` protects the scheduler's bookkeeping:

| Field | Purpose |
| --- | --- |
| `owner: Option<GlobalTxId>` | Identifies the transaction currently admitted to the slot. |
| `remote_waiting: BTreeSet<WaitKey>` | Orders waiters coordinated on other databases. |
| `local_waiting: BTreeSet<WaitKey>` | Orders waiters coordinated on this database. |
| `wait_entries: HashMap<u64, WaitEntry>` | Maps each wait ID to its transaction and `Arc<Notify>` wakeup object. |
| `waiter_ids_by_tx: HashMap<GlobalTxId, u64>` | Finds an existing waiter registration by transaction ID. |
| `wounded_owners: HashSet<GlobalTxId>` | Records owners selected for the wound flow to suppress duplicate initiation. |
| `next_wait_id: u64` | Allocates IDs for waiter registrations. |

The ordered sets choose who goes next; the maps find registrations and their
wakeup objects. A `WaitKey` sorts first by transaction ID and then by wait ID.
The scheduler chooses the first remote waiter if any exist, otherwise the first
local waiter. “Local” is determined by comparing `tx_id.creator_db` with
`local_database_identity`.

### Acquisition and cleanup

`acquire(tx_id, on_wound)` returns either `Acquired(GlobalTxLockGuard)` or
`Cancelled`. Its normal flow is:

1. Subscribe to the requester's wound signal and reject a missing, wounded,
   aborting, or terminal session.
2. Under the scheduler mutex, check ownership. If the slot is free and this
   requester is next, remove its waiter entry, set `owner`, and return a guard.
   Stale entries at the head of the queue are pruned before admission.
3. Otherwise, find or create the requester's waiter registration. If an eligible
   older remote requester encounters a younger owner, record that owner in
   `wounded_owners` and select it for the wound flow.
4. Release the mutex before waiting through the grace period. Recheck that the
   same owner still holds the slot before marking it wounded locally when
   permitted and spawning the supplied `on_wound` callback. Prepared participants
   receive the coordinator notification without a local wound, as described below.
5. Await either the requester's wound signal or its waiter notification, then
   recheck the state. A notification means ownership may be available; it does
   not itself grant the slot.

The mutex is held for bookkeeping, not across asynchronous waits or network
work. The callback connects the scheduler to the host's coordinator-notification
logic; the manager itself does not implement the wound RPC.

Two guards cover different lifetimes. `WaitRegistration` removes the queued
entry if the acquisition future is cancelled or dropped; if that entry was the
head and the slot is free, cleanup wakes the next waiter. Successful acquisition
removes and disarms that registration. `GlobalTxLockGuard` then represents
ownership: dropping it calls `release()`, clears the matching owner and its
wound-tracking entry, and notifies the next waiter.

Ownership release, session removal, and prepare-mapping removal are separate
operations. Callers must coordinate those lifetimes with rollback and 2PC
completion. Similarly, session state setters store a state without enforcing a
complete transition graph. The manager provides tracking, admission, and
cancellation signals; surrounding host code performs rollback, decision delivery,
and recovery.

## The important 2PC boundary: prepared participants

An executing participant can be marked wounded locally and stop cooperatively.
But a **prepared participant must not independently abort just because another
transaction wants its slot**. It has already promised to follow the coordinator's
decision, which might be commit.

The scheduler's `should_wound_locally` check explicitly excludes sessions whose
role is `Participant` and whose state is `Prepared`. For those owners, it sends
the wound toward the coordinator without setting the local wound flag. The
participant retains ownership until the decision path resolves it.

The coordinator's `wound_global_tx` handler verifies that it is the coordinator,
treats unknown or already committed/aborted/aborting sessions as no-ops, and
otherwise marks the session wounded and transitions it to `Aborting`. It aborts
locally prepared work when present and sends abort requests for known participant
prepare handles.
Remote wounds use `POST /v1/database/:name_or_identity/2pc/wound/:global_tx_id`,
routed to `GlobalTxId.creator_db`.

The safety requirement is that a wound cannot reverse an established commit
decision. The prepared-participant guard and coordinator state checks are
relevant mechanisms, but do not by themselves prove every commit/wound race
correct. Such races must be evaluated with the decision and recovery paths.

## Cancellation, failures, and durability

Wounds use cooperative cancellation rather than forcibly killing execution.
Wound checks and notifications allow running work, blocked remote calls, and
waiting admission requests to observe cancellation. Cleanup must roll back
uncommitted work and release ownership; simply returning an error while leaving
a prepared participant locked would leave the original conflict unresolved.

An RPC timeout is not permission for a prepared participant to guess an abort.
If the coordinator cannot be reached, decision resolution and recovery remain
necessary. Wound-wait addresses lock contention; it does not remove 2PC's
dependence on coordinator decisions. The prototype logs participant abort
transport failures, so successful delivery cannot be assumed from sending a wound.

This branch also enables `fake_2pc_persistence` by default. That option bypasses
durability waits in the 2PC execution paths for experimentation. It is separate
from wound-wait: contention results with that option enabled do not establish
crash-safe atomicity for the durable protocol.

## Code map

Paths below refer to the prototype revision named above. To inspect that exact
version without switching branches, use `git show 38f335d0ea:<path>`.

| Path | What to read |
| --- | --- |
| `crates/lib/src/tx_id.rs` | Global ordering, propagated header, retry identity. |
| `crates/core/src/host/global_tx.rs` | Sessions, admission, grace period, prepared-participant protection, scheduler tests. |
| `crates/core/src/host/module_host.rs` | Admission integration, coordinator wound handler, wound RPC dispatch. |
| `crates/core/src/host/instance_env.rs` | Cross-database reducer calls. |
| `crates/core/src/host/wasm_common/module_host_actor.rs` | Reducer execution and 2PC completion/durability paths. |
| `crates/core/src/client/client_connection.rs` | Wounded-attempt retry handling. |
| `crates/client-api/src/routes/database.rs` | HTTP 2PC and wound endpoints. |
| `crates/core/src/config.rs` | Grace-period and fake-persistence configuration. |

## Verification criteria

These criteria preserve the original plan's acceptance goals and incorporate
later protocol refinements. They are requirements to verify, not a claim that
all cases have passed at the documented revision.

- `/call` and `/prepare` propagate the same `GlobalTxId`, and all databases
  agree on its ordering and coordinator identity.
- An eligible older remote requester wounds a younger owner; younger requesters
  wait behind older owners. Local admission follows the exceptions above.
- An owner finishing within grace avoids a wound. Cancelling a waiter or
  releasing ownership removes stale scheduler state and wakes eligible work.
- Distributed lock cycles are broken when cancellation and decision delivery
  complete. A wounded attempt aborts globally, and an older transaction can
  proceed without manual intervention under those conditions.
- Prepared participants defer to the coordinator. A wound cannot reverse an
  established commit decision, including when prepare, commit, and wound race.
- Cooperative cancellation is observed before remote calls and prepare/commit
  work, and after reducer execution. Abort cleanup releases local ownership
  and surfaces a retryable wounded outcome where appropriate.
- Repeated wound and abort requests are safe and idempotent; late replies do
  not leave prepared work or scheduler ownership stranded.
- Recovery preserves enough transaction identity and decision information to
  resolve prepared work after crashes or decision-delivery failures.
- Existing 2PC success and failure cases continue to pass. Durable recovery is
  validated with `fake_2pc_persistence` disabled.

Scheduler unit tests cover several local cases. Distributed cycles, atomicity,
commit/wound races, and recovery also need end-to-end validation.

## Alternative: centralized lock coordination

An alternative is to consult a centralized coordinator before acquiring each
lock. Transactions report their lock requests to that coordinator, which decides
whether each acquisition may proceed. Tyler prototyped this approach using
SpacetimeDB itself as the coordinator.

A shared coordinator can maintain a global view of lock ownership and pending
requests, putting the admission decision in one place. This is a different role
from the per-transaction 2PC coordinator: lock admission determines whether work
may proceed, while 2PC still determines whether the distributed transaction
commits or aborts.

The tradeoff is an additional coordination dependency on the lock-acquisition
path. A dedicated coordinator service or SpacetimeDB database must be operated,
and requests from otherwise independent databases converge on it. That can
introduce a centralized point of contention and coordination latency.

The distributed wound-wait approach described in this document requires no
additional services or databases. Each database manages its own admission state
and contacts the relevant transaction coordinator when a wound is needed, so it
introduces no centralized point of contention shared by all participating
databases. Contention still exists within each database and between transactions
accessing the same databases; the tradeoff is that wound propagation, cancellation,
and 2PC decision handling must be coordinated across those databases.
