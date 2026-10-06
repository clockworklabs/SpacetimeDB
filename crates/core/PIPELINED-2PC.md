# Pipelined two-phase commit with speculative execution

Status: exploratory design proposal. This explains one possible approach and the
questions that would determine whether to pursue it. It is not an implementation
specification, a description of implemented behavior, or a completed correctness proof.
The starting point is the prototype documented in
[Distributed Wound-Wait](DISTRIBUTED-WOUND-WAIT.md), particularly
`jsdt/2pc-fake-durability` at `38f335d0ea`.

## Objective and proposed approach

Release database execution locks once a distributed transaction has finished
executing, rather than holding them through log persistence and the coordinator's
commit decision. Later transactions may execute against its tentative effects,
but may not commit or expose state-dependent results until their dependencies commit.

The proposal has a locking/execution stage followed by asynchronous 2PC. These
are not two independent commit decisions: staging grants permission to execute
ahead, while one durable coordinator decision determines the transaction's fate.

The baseline explored here uses database-wide execution slots and conservative
dependencies on preceding speculative transactions. It requires recoverable
speculative state, delayed external visibility, and cascading aborts.
It does not require a new centralized service or coordinator database: the
originating database coordinates each transaction.

The performance hypothesis is that overlapping execution with durability improves
completed transaction throughput on a hot database. Shorter execution lock holding
does not establish that benefit by itself: dependent commit decisions still
serialize in this baseline. Transaction response latency still includes durability
and decision delivery; pending work and recovery costs increase. The design does
not eliminate locking during distributed execution or 2PC blocking during
coordinator failure.

The protocol below makes conservative choices so there is a concrete example to
reason about: participants wait for predecessor commits before voting yes, clients
wait for finalization, and rollback invalidates a speculative suffix. Those are
starting points to test, not settled product or storage decisions. Message names,
record layouts, and runtime structures are illustrative.

Some requirements are safety obligations for any variant: recover the exact work
that was voted on, preserve one authoritative outcome, and prevent observations
based on aborted speculative state. Other requirements belong to this particular
baseline, such as the placement of the dependency wait. An alternative may change
those rules, but needs its own ordering, recovery, and visibility argument. The
[experiments below](#experiments-to-guide-the-design) are intended to guide that
choice before a broad implementation.

## Scope and assumptions

- One fenced authority controls each database's log and transaction decisions.
  A replacement authority recovers the authoritative history before serving
  decision requests. A stale authority cannot continue writing or issuing votes.
- Durable storage preserves acknowledged records across the failures we support.
  “Persisted” means the configured recovery guarantee, including replication if
  required, not merely an append to a process buffer.
- Reducers operate on transactional database state. External effects must be
  deferred until commit or mediated by an outbox with its own delivery semantics.
- Every operation touching the speculative database state participates in this
  protocol, including single-database writes, scheduled work, and relevant reads.
  Schema changes and other unsupported operations must drain the pipeline first.
- Transactions can discover participants dynamically while executing, but their
  participant set and work are immutable once execution closes.
- Network messages may be delayed, duplicated, reordered, or lost. Timeouts
  trigger retries and status resolution, not an invented commit or abort outcome.

The baseline does not attempt row-level dependency tracking, lock-free
cross-database snapshots, conditional global commits, or continued execution by a
transaction after it releases any execution lock.

## Three different milestones

| Milestone | Meaning | Can successors execute? | Can clients observe success? |
| --- | --- | --- | --- |
| Staged | Effects and predecessor dependencies are frozen; execution ownership has been released. | Yes, with dependencies. | No. |
| Prepared | Recovery data and a durable yes vote exist; this participant's predecessor commits are resolved successfully. | Yes. | No. |
| Committed | The coordinator's commit decision is durable. | Yes. | After the visibility/finalization rules below. |

A staged record can already be physically durable while its transaction is
still logically undecided. Persistence and commitment must be represented
separately throughout the datastore, logging, and subscription APIs.

The immediate reply called `prepared pending{x}` in the initial notes is named
`StagePending` here to avoid confusing it with a durable 2PC yes vote. It identifies
the attempt and local stage record to watch. It is never sufficient evidence for
the coordinator to commit.

## Identities and dependencies

Use the full `GlobalTxId`, including its attempt, as the transaction-attempt key.
Retries preserve the logical transaction's age but use a distinct attempt. The
implementation must guarantee that attempts cannot be reused after restart;
extend the existing allocation scheme with durable allocation or an incarnation
identifier if necessary. This requirement is separate from log-position identity.

Proposed types, independent of wire encoding:

```text
TxAttempt = globally unique transaction identity including retry attempt
LogRef    = (database, log_generation, sequence_number)

Persisted(LogRef)
Prepared(database, TxAttempt, stage_identity)
Committed(TxAttempt)
```

`LogRef` identifies a particular record in a particular history. A generation
changes when a suffix can be replaced with different records at reused sequence
numbers; surviving records retain their original identities. An equivalent
record-identity scheme is acceptable if the storage layer already provides one.
A sequence number from an abandoned proposal must never resolve against a new
record that happens to reuse its number.

`Persisted` is resolved by the referenced database's authoritative durability
layer. It requires that the record exists and belongs to the recoverable history.
A contiguous durable-prefix watermark can prove it only within the appropriate
history. Allocating a sequence number, receiving a proposal, or observing a larger
number in a history with holes is insufficient.

For transaction T on database B, the baseline uses these conditions:

```text
eligible_to_vote_yes(B, T) =
    staged_effects_and_dependencies_are_persisted(B, T)
    AND every predecessor U in dependencies(B, T) has Committed(U)
    AND no rejection/abort has won the local state transition

Prepared(B, T) =
    eligible_to_vote_yes(B, T)
    AND the local yes-vote record is persisted

eligible_to_commit(T) =
    Prepared(B, T) for every B in T's closed participant manifest

Committed(T) =
    eligible_to_commit(T)
    AND the coordinator's Commit(T) decision is persisted
```

The coordinator's own application work counts as a participant. Dependencies of
T's prepare are predecessor transactions, never `Committed(T)` itself. Later work
depends on T; T cannot depend on its own outcome.

`Committed(T)` means an irrevocable decision with recoverable participant work.
It does not mean every participant has already received that decision or updated
its externally readable state.

## Execution ordering and dependency capture

During execution, retain the existing database-wide admission model and
wound-wait conflict handling. A participant returns intermediate reducer results
to the coordinator while retaining ownership and tentative local work.

The coordinator closes execution only after all nested calls have returned,
all reducer work and application validation have succeeded, and the complete
participant manifest is known. No outstanding call may later add work or a
participant. After closure, no participant may execute more code or acquire
another lock for this attempt. A manifest digest and per-participant stage identity
bind messages to this frozen work; duplicate messages cannot alter it.

When a new transaction U acquires B's slot, it captures dependencies on all
unresolved staged predecessors whose state it may use. For the baseline,
assume every preceding speculative application transaction on B matters, including
overwrites and operations that happen to touch unrelated rows. This deliberately
overapproximates read/write conflicts. Repeated calls by the same transaction
reuse its held ownership and accumulate work before closure.

This describes the logical dependencies, not necessarily their stored encoding.
Explicit predecessor sets are easy to inspect, but a chain of depth k can require
O(k²) total references if each entry lists all earlier unresolved entries. A
reference to the immediate local predecessor might encode the same transitive
constraint with O(k) references. That option needs a proof that commit ordering,
abort propagation, and recovery preserve the chain through finalization and
cleanup. Compression would reduce metadata, not the number of transactions
invalidated by an ancestor abort. Finer-grained conflict tracking could reduce
false dependencies and abort fanout, at a larger implementation and proof cost.

Closing execution before exposing tentative effects supplies the ordering argument:
if U depends on T because it accessed state exposed by T, T had already closed
execution before that access, and U closes afterward. Dependency edges therefore
point toward earlier execution closures. Closure is a protocol event, not a
synchronized wall-clock timestamp.
Two-phase locking supplies the conflict-order argument; dependency checks prevent
successors committing before their predecessors. This is a proof sketch to verify
in the protocol model, not a substitute for one.

Releasing a lock while still acquiring locks elsewhere violates this argument.
For example, T could expose state to U and then wait for a database U holds while
U waits for T's commit. That more aggressive design is out of scope.

## Protocol sketch

### 1. Execute and close the participant manifest

The originating database coordinates T. It gathers intermediate results and
participant identities while retaining transaction ownership at each database.
Ordinary reducer failures, validation errors, and wounds can abort this phase.
An application failure derived from speculative reads is itself tentative; aborting
the attempt does not make that failure safe to return to the client. See the
[visibility discussion](#visibility-retention-and-bounded-speculation).

On success, it freezes the manifest and sends `StageAndPrepare(T, manifest)` to
every participant. Record the coordinator descriptor in its log without requiring
a separate synchronous flush before sending. Every participant's stage record
also identifies the coordinator and frozen manifest, so recovery does not depend
on that early descriptor having survived.

The coordinator serializes execution closure, incoming wounds, and eventual
decision selection. In-flight wounds may still cause a participant that has not
staged to reject the request; partial staging is handled by abort below.

### 2. Stage locally and release ownership

Under the participant's execution and protocol-state synchronization:

1. Verify the attempt, manifest, ownership token, and completed local work. Reject
   a stale request or an attempt already wounded/rejected.
2. Freeze the local effects and predecessor dependency set.
3. Enqueue a stage record containing those effects and dependencies. Do not wait
   for disk or remote dependency resolution.
4. Install the tentative state and dependency barriers as one local handoff.
   A successor must never see the effects without inheriting their dependencies.
5. Transition to `Staged`, release both datastore execution ownership and scheduler
   admission, and reply `StagePending(T, stage_ref)`.

No network or durability wait occurs in this handoff. The log append queue must
preserve ordering so a successor's persisted stage cannot omit the local recovery
history it relies on. A crash before persistence may lose both tentative work and
the pending acknowledgement; it cannot lose a durable yes vote.

### 3. Persist and produce a durable yes vote

A background resolver waits for the stage record to persist and for every
predecessor to commit. Successful observations must be recoverable: persist local
decision evidence before, or atomically with, the yes-vote record.

Then append `Prepared(T, stage_ref)` and wait for its persistence. Only now emit
`Prepared(B, T, stage_identity)` to the coordinator. Stage and vote records can
share a durability batch when all prerequisites are already satisfied. Do not
assume that every record needs its own flush.

If a predecessor aborts or staging cannot be recovered, serialize a durable local
`Reject(T)` against the yes-vote transition and report failure to the coordinator.
A participant must never both reject and vote yes for the same stage identity.
When storage is unavailable, it must stop voting; recovery establishes which
transition survived before it resumes.

The coordinator receives votes by notification or idempotent status polling;
an HTTP request need not remain open throughout the wait. A participant retains
its staged data and state even after replying `StagePending`.

### 4. Make one durable decision

When every participant has voted yes for the matching manifest and stage identity,
the coordinator appends `Commit(T, manifest)` and waits for persistence. It then
announces commit to participants and dependency subscribers.

If a participant rejects, or the coordinator chooses abort before committing, it
instead persists `Abort(T, reason)` and broadcasts that outcome. After all
participants have staged, routine contention and client cancellation must not
be reasons to abort; failure or inherited dependency failure remain reasons.

Decision selection is serialized. Once a commit write is in flight, a write
timeout is not permission to append an opposing abort. Recover the authoritative
log and fence the old writer before deciding what happened. A durable commit is
never reversed.

### 5. Observe and finalize

Participants persist `Observed(T, decision, decision_ref)`, update the application's
resolved state, and release local dependency barriers. They send a `Finalized`
acknowledgement only when the observation and recovery state are durable and the
local committed view has caught up with that decision.

In the baseline, return client success after the commit decision and all
participant finalization acknowledgements. This is conservative and adds
response latency, but does not extend execution lock holding. A later optimization
may acknowledge at the durable coordinator decision if subsequent reads are
explicitly made to resolve outstanding decisions and honor that acknowledgement.

Notifications may be batched. Duplicate staging, votes, decisions, observations,
and acknowledgements are idempotent; a mismatched manifest or stage identity is
an error, not another execution of the reducer.

## How long a remote database remains locked

```mermaid
sequenceDiagram
    participant C as Coordinator
    participant B as Database B
    participant L as B durability
    C->>B: Execute T
    Note over B: Acquire execution slot; execute
    B-->>C: Intermediate result; retain ownership
    Note over C: Finish all execution; close participant manifest
    C->>B: StageAndPrepare T
    B->>L: Enqueue stage and dependency records
    Note over B: Install tentative state; release ownership
    B-->>C: StagePending T
    Note over B: Execute successors with dependencies on T
    L-->>B: Stage persisted
    Note over B: Resolve predecessor commits; persist yes vote
    B-->>C: Prepared T
    Note over C: All votes received; persist Commit T
    C->>B: Commit T
    Note over B: Persist observation; finalize application state
    B-->>C: Finalized T
```

Lock duration is local execution, plus remaining distributed execution after B
returns, plus communication to deliver the staging request, plus local staging.
For an early participant, this includes later sequential remote calls. For the
last participant, it is approximately its execution plus a coordinator round trip
and staging overhead. It excludes prepare persistence, predecessor commitment,
coordinator decision persistence, and decision delivery.

## Remaining limit: dependent commits still serialize

Early lock release pipelines execution and stage persistence, but the baseline
protocol still waits for `Committed(T)` before a dependent U can issue all of its
yes votes. A long chain of transactions on a hot database can therefore accumulate
speculative work faster than its final decisions resolve. Grouping stage writes
does not, by itself, remove that chain of durability and communication waits.

This is an explicit limit of the conservative proposal. Benchmark both execution
throughput and completed commits per second; lower lock hold time is not enough
to establish a sustainable throughput improvement. Backpressure will eventually
expose any slower decision-processing bottleneck.

For an already-staged successor U, the baseline still has a critical path like:

```text
Commit(T) durable -> predecessor notification -> Prepared(U) durable
                 -> vote delivery -> Commit(U) durable
```

When reducer execution is short relative to these waits, speculative execution
may mostly grow the backlog. Database-wide dependencies also mean that unrelated
rows can become blocked behind a transaction involving a slow remote database.
The size of the useful overlap is a workload question, not an established result.

There are several directions to investigate if this limits the benefit:

| Option | Possible benefit | Uncertainty or additional obligation |
| --- | --- | --- |
| Keep dependency checks before participant yes votes, as above. | Simplest baseline: a yes vote already establishes predecessor success. | Successive dependent transactions retain both vote and decision persistence waits. |
| Prepare recoverable effects earlier; check dependencies at the coordinator before committing. | Participant prepares could batch while predecessors are undecided, removing some work from the dependent decision path. | The coordinator needs the complete, frozen dependency set and recoverable evidence of predecessor commits. Prepared participants must retain work and await an authoritative abort even if an ancestor aborts. This changes the vote invariant used elsewhere in this document. |
| Order related decisions in a shared log, where coordinator placement permits it. | Could let dependent decisions share a durability batch. | Requires a common durable order and revised dependency validation; transactions spanning different decision logs still need a solution. Changing placement or adding a shared service has its own operational costs. |
| Persist conditional readiness or decision records across separate logs. | Could overlap more of the distributed decision path. | A durable conditional record is not an unconditional commit. Recovery must determine dependency outcomes and propagate aborts without exposing premature success. |

Moving the dependency check to the coordinator would still serialize dependent
commit decisions; it does not reproduce shared-log batching by itself. Across
different coordinator logs, persistence of U's record does not prove persistence
of T's decision. None of these alternatives is obtained safely by merely skipping
a wait in the baseline. A timing model can help decide which, if any, warrants a
separate protocol model.

## Options for speculative application state

The datastore needs to distinguish the committed read view, frozen speculative
work, and the transaction currently executing against the speculative tip. These
are logical roles; they need not be three complete copies of the database. The
existing `MutTxId` holds a write lock on `CommittedState`, and its commit path
merges transaction effects into that state. Separating execution ownership from
distributed commitment therefore requires a datastore design, not only a change
to the scheduler guard's lifetime.

| Possible representation | What makes it attractive | What to measure or establish |
| --- | --- | --- |
| Separate committed and speculative states, with immutable redo between them. | A straightforward way to demonstrate committed reads, ordered finalization, and suffix reconstruction. | Memory for both states, repeated application of effects, index maintenance, and rebuild time. |
| Shared immutable versions or copy-on-write state. | Could preserve committed and speculative views without copying unchanged data. | Copy granularity, index/blob/sequence handling, version retention, and snapshot integration. Existing support and acceptable overhead are not yet established. |
| A committed base plus ordered delta overlays. | Could make retained speculative data proportional to the writes. | Reads and constraint checks must see the correct combined state; deep overlays, deletes, and index scans may be expensive. |

These approaches can be combined. A small experiment should establish the cost
of staging, reading, finalizing, and rolling back before choosing a representation.
Protocol metadata must also remain writable and recoverable while application
work is unresolved; reusing transactional system tables cannot make decision
processing depend on the application transaction it needs to resolve.

Single-database work is an important comparison. The ordinary `commit_tx` path
already releases its mutable transaction before submitting durability work.
Participating in dependency and visibility tracking need not mean paying a full
distributed protocol when there are no remote participants or unresolved
predecessors. An optimized local path is a candidate, provided it preserves
ordering when mixed with distributed speculative work. Its benefit and complexity
should be measured alongside the distributed case.

## Log records and local data structures

The following are logical records; encoding and batching are implementation choices.

| Record | Meaning |
| --- | --- |
| `Stage(T, manifest, effects, dependencies)` | Recoverable frozen tentative work. Include deterministic redo data or an equivalent immutable version, not just reducer arguments. |
| `DependOn(T, predecessor, start_position)` | Optional separate encoding of a dependency; must be recoverably bound to the stage/effects it protects. |
| `Prepared(T, stage_ref)` | Participant's durable yes vote; predecessor success evidence must survive with it. |
| `Reject(T, stage_ref, reason)` | Local refusal before voting yes; prevents a delayed prepare from reviving invalid work. |
| `Commit(T, manifest)` / `Abort(T, reason)` | Authoritative coordinator decision. |
| `Observed(T, decision, decision_ref)` | Local durable knowledge of an authoritative outcome. |
| `FinalizedAck(T, participant)` | Optional durable coordinator bookkeeping for later retention/cleanup. |

Persisting effects avoids re-running reducers against a different speculative
state after recovery. If deterministic replay is chosen instead, all inputs,
schema/code versions, reads, and nondeterministic results needed to reproduce the
exact stage must be retained. Reducer arguments alone are insufficient.

Suggested runtime and recoverable indexes:

| Structure | Key and essential fields |
| --- | --- |
| Participant transactions | `T`: coordinator, manifest digest, stage identity and position, local state, effects/version reference, predecessor set, vote/decision evidence. |
| Coordinator transactions | `T`: closed manifest, expected stage identities, received votes, decision state/reference, finalization acknowledgements. |
| Needed commits | `(local application position, predecessor T)`: work blocked by that predecessor, with a reverse index from T to its dependents. |
| Needed prepares | `(coordinated T, participant)`: expected stage identity and outstanding durable yes vote. |
| Known decisions | `T`: authoritative outcome and durable observation/reference. |
| Speculative application queue | Local ordered application transactions, immutable effects, dependencies, and external events awaiting release. |

The existing `GlobalTxManager` remains responsible for execution admission and
running-attempt wounds. Split its execution guard lifetime from the longer
participant protocol lifetime. Dropping the guard at staging must not remove
the session, prepare mapping, recovery data, or dependency registrations.
Add explicit `Staged` and vote-persistence states rather than overloading the
existing `Prepared` state with an in-memory acknowledgement.

One local state sketch is:

```text
Executing -> Staged -> VotePersisting -> Prepared -> CommitObserved -> Finalized
    |           |
    +-----------+-> Rejected/AbortObserved -> CleanedUp

Prepared -> AbortObserved -> CleanedUp   (only an authoritative abort decision)
```

Per-attempt synchronization must serialize vote persistence, rejection, wounds,
and decisions. Independent mutex-protected assignments to an enum are insufficient
to implement these transitions safely.

## Physical persistence versus resolved application progress

Maintain separate physical and logical progress:

- The physical log watermark records the contiguous recoverable log prefix.
- The resolved application frontier records how far application transactions have
  known outcomes and their effects have been applied or skipped accordingly.

For example:

```text
100: Stage T
101: Stage U, depends on T
102: Stage V, depends on T and U
103: Observed Commit(T)
104: Prepared U
```

All five records can persist while U and V remain undecided. Recovery must scan
protocol records beyond unresolved application entries. It must not stop at 101
and refuse to discover the observation at 103 that permits progress.

Likewise, a coordinator's decision for an earlier transaction may be appended
after unrelated speculative application work. Protocol records must not inherit
that work's application dependencies merely because they occur later in the log.
Otherwise a later transaction can prevent the record that resolves its own
predecessor from being persisted or processed.

A barrier begins after the stage that creates it. Capture each transaction's
predecessors explicitly in its recoverable descriptor; do not derive them solely
from the current contents of a mutable `needed commits` table. After a dependency
resolves, its live index entry can disappear only when replay or a snapshot still
preserves enough information to interpret earlier records.

## Aborts, wounds, and speculative rollback

The target is that after every participant has successfully staged, aborts are
exceptional: lost unprepared state, persistence failures, or predecessor aborts.
A crash need not abort a transaction whose prepared state survives. A timeout
alone is never evidence of an abort, and a committed transaction must recover
to commit.

There is a partial-staging window. One participant may already have released its
lock while another is still handling a prior wound or rejects staging. The former
may already have speculative successors, which must then abort. Thus “aborts only
on crashes” is a steady-state objective after successful global staging, not a
guarantee as soon as the first `StagePending` reply arrives.

Once a participant is staged, ordinary wounds cannot invalidate it locally:
it has no execution ownership to release. A late wound must be evaluated against
the coordinator's serialized phase and any genuine earlier rejection. A prepared
participant can only follow the authoritative decision.

When T aborts:

1. Stop new execution against affected speculative state.
2. Mark local descendants invalid and prevent them from voting yes. Inform their
   coordinators so the abort propagates across databases.
3. Resolve each distributed attempt through its authoritative decision path;
   a local invalidation is not a fabricated remote decision.
4. Restore the committed application state and rebuild any surviving speculative
   work in order. Retry aborted logical operations with new attempt identities.

Descendants may still be executing when invalidation starts. The implementation
must prevent their stale work from being staged into the rebuilt state, for
example by draining cancelled execution before rebuilding or by checking a state
generation at handoff. Which mechanism fits the datastore is open; the protocol
model should exercise this race rather than considering only frozen descendants.

Database-wide dependencies make suffix invalidation a simple first strategy:
every later speculative application entry on the affected database is dependent.
The implementation can rebuild from a committed snapshot plus retained redo
instead of selectively undoing arbitrary overlapping updates.

Do not truncate the physical log indiscriminately. Its suffix may contain durable
commit decisions for other transactions and observations needed for recovery.
Invalidate application entries and replay protocol metadata separately.

Under the dependency rules, a descendant cannot have voted yes at the database
where it depends on an unresolved ancestor that later aborts. Other participants
of that descendant may already be prepared and must await its coordinator's abort.
Finding a globally committed descendant of an aborted ancestor is an invariant
violation, not a case ordinary cascading rollback may silently repair.

## Recovery and authoritative status

On restart, fence old authority, recover the physical log and snapshots, rebuild
protocol indexes, and then reconstruct committed and speculative application
state. Do not publish a replayed speculative row merely because its stage record
was persisted.

| Recovered condition | Required behavior |
| --- | --- |
| Pending stage acknowledgement, but stage absent | No durable yes vote could legitimately exist. Reject that lost stage under recovered authority; never substitute a new stage at a reused position. |
| Stage present, no yes vote | Resolve predecessors and resume preparation if valid, or reject. Serialize that choice with duplicate requests and recovered decisions. |
| Durable yes vote, decision unknown | Retain recovery data and query the coordinator. Do not abort on a timeout. |
| Coordinator commit write had uncertain completion | Recover its log before selecting or reporting an outcome. |
| Durable commit decision present | Resend commit; participants reconstruct and finalize the exact staged effects. |
| Durable abort decision present | Resend abort and invalidate dependent work. |
| Coordinator descriptor present, no final decision | A conservative recovered coordinator may durably choose abort after fencing prior decision makers. |
| Coordinator reports unknown | Treat as unresolved unless a recovered, fenced authority records an abort or an explicitly defined retention proof establishes the outcome. |

A participant that has retained a valid durable yes vote can re-send it after
restart. Coordinator recovery need not persist every incoming notification if it
can query participants again; the final decision and its closed manifest must
be recoverable before they are exposed as authoritative.

For an unknown attempt discovered through participant recovery, the fenced
coordinator may durably record an abort after establishing that its complete
authoritative history contains no commit. It must reject later stale requests for
that attempt. Absence from an in-memory table or an incomplete log scan is not
sufficient evidence. Permanent loss of acknowledged durable history lies outside
the crash model and requires operational recovery, not guessing a decision.

Snapshots must contain a consistent cut of application state and protocol
metadata, including pending stages, dependencies, log identities, votes, and
retained decisions. A snapshot must not classify the speculative execution tip
as committed state.

## Visibility, retention, and bounded speculation

Buffer reducer success, subscription updates, and scheduled/external effects
until their transaction and predecessors are committed. Internal reducer results
may travel between participants before commit only within a tracked transaction.
Queries must explicitly choose either a committed local view or tracked
speculative execution whose result waits for dependencies.

State-dependent failures need a visibility barrier too. Suppose T tentatively
inserts a key, U sees it and fails with “already exists,” and T then aborts.
Returning U's error would expose a result based on state that never committed.
Rolling back U's writes does not validate that result. Dependencies must remain
tracked for this purpose even if U fails before reaching staging.

One policy would buffer the error until its predecessors commit, and retry U with
a new attempt if a predecessor aborts. Another would return a transaction-abort
outcome that does not expose the speculative application result, leaving retry to
the caller. Retry limits and the client contract remain open. Failures demonstrably
independent of speculative state, such as malformed requests rejected before
execution, need not wait. Existing reducer-failure delivery paths must be included
in the visibility work, not just successful reducer and subscription messages.

Independent local committed reads do not provide a consistent cross-database
snapshot. Multi-database reads requiring transactional consistency must participate
in the protocol; a distributed snapshot scheme is separate work. Asynchronous
subscription delivery also does not imply simultaneous visibility across databases.

An early prototype could retain coordinator decisions and rejection tombstones
without automatic garbage collection. Participant finalization acknowledgements
alone do not prove that every downstream dependent has persisted its observation.
Bounded retention requires tracking those references or a durable checkpoint/epoch
scheme proving that no live attempt, recovery record, or delayed request needs the
old outcome. This is required before production use.

Speculation needs bounds on bytes, transaction count, dependency depth, and
outstanding external-event buffers. Reserving staging capacity before execution
would avoid rejecting completed staging under ordinary memory pressure, but work
size and participants may be discovered dynamically. Candidate policies include
reserving a maximum transaction budget, acquiring capacity incrementally with
failure handled before execution closes, or spilling frozen effects to storage.
The last option could put storage waits back into staging. The appropriate policy
and resource bounds are open.

Whatever the policy, throttling admission must leave enough resources for log
flushing, decision RPCs, dependency resolution, and recovery. Backpressure must
never block the control work needed to drain the pipeline. In the baseline,
administrative cancellation after staging detaches the client while the
transaction resolves normally.

## Experiments to guide the design

The useful first step is to test the safety and performance hypotheses with small
models and prototypes. A full datastore integration should not be necessary to
discover that the dependent decision path dominates the expected benefit.

1. **Model the ordering and failure rules.** Use two or three databases with
   explicit stage, vote, and decision persistence; reordered or duplicated messages;
   crashes; and fenced replacement authorities. Include partial staging plus a
   wound, rollback while a descendant is executing, and protocol records behind
   unresolved application work. Check agreement, dependency ordering, visibility,
   and eventual draining when authorities and storage recover. Model compressed
   dependencies or a different placement of dependency checks as separate variants
   with explicit invariants. This should reveal missing rules before the choice
   of data structures makes them expensive to change.

2. **Estimate and then measure sustainable throughput.** A timing model can vary
   reducer cost, network delay, storage latency, batching, and coordinator placement
   to estimate the benefit of overlap. Follow it with a small harness using actual
   durability, comparing lock-through-commit against the baseline. Cover a hot
   database with short reducers, longer reducers, disjoint databases, unrelated
   rows on the same database, and mixed local/distributed traffic. Use both one
   coordinator and several coordinators. Hold backlog limits fixed, run beyond the
   initial pipeline fill, and measure completed commits, latency percentiles, and
   queue depth. If the gain is only more queued work, that is evidence to revisit
   dependency granularity or decision ordering before broader integration. Synthetic
   timing results guide the experiment; fake-persistence runs cannot validate the
   durable throughput claim.

3. **Compare speculative-state representations.** Implement only enough of one or
   two candidates to stage effects, serve committed and speculative reads, finalize
   a prefix, and invalidate a suffix. Vary database size, write size, index count,
   and speculation depth. Include inserts, deletes, uniqueness checks, sequences,
   and snapshot/restart. Measure retained memory, bytes copied or replayed, read
   cost, and rebuild time. This determines whether redo, shared versions, overlays,
   or a combination is plausible. Compare dependency-set and predecessor-chain
   metadata at increasing depth as part of the same experiment.

4. **Exercise stalled and failing pipelines under resource limits.** Delay one
   coordinator, fill the speculative budget, then restore progress or abort the
   oldest unresolved ancestor. Include transactions discovering another participant
   after admission. Measure abort fanout, time to resume useful work, and memory
   during reconstruction, not just steady state. Check that control work continues
   with application admission stopped. The result should guide admission budgets,
   capacity reservation, and whether the coarse dependency policy is acceptable.

5. **Test the externally observable contract and retention.** Try the tentative
   insert/“already exists” example, subscriptions starting while stages are pending,
   reads immediately after acknowledged success, and snapshots containing unresolved
   work. Compare waiting for finalization with any proposed earlier acknowledgement.
   For decision cleanup, delay a dependency observation or old request across a
   checkpoint and restart. These cases determine which visibility barriers and
   retained evidence the client contract requires, and whether a proposed retention
   scheme is safe. A short throughput run cannot establish bounded storage use.

The outcome could be to keep this baseline, change a particular choice, narrow
the workloads that use speculation, or defer it if the benefit is too small.
The experiments are decision inputs, not predictions that the approach will win.

## Possible implementation path and verification

If the experiments justify proceeding, a possible sequence is to add versioned
protocol records and recovery indexes while retaining current execution behavior;
introduce the chosen committed/speculative state representation; then split
execution ownership from participant lifetime and connect the coordinator path.
Bounds, visibility barriers, cascade recovery, and snapshot support need to be
present before testing this as a general workload path. Safe decision retention
is required before production use. The exact decomposition depends on which
representation and protocol variant the experiments support.

Likely integration points are `host/global_tx.rs` for admission/state lifetimes,
`host/wasm_common/module_host_actor.rs` for participant execution and preparation,
`db/relational_db.rs` for speculative/committed views, `db/durability.rs` and
`crates/commitlog` for record identities and persistence, and subscription/client
paths for delayed visibility. These changes extend beyond `GlobalTxManager`.

Safety checks for the baseline (revisit variant-specific invariants if it changes):

- At most one authoritative decision per attempt; no commit without all matching
  durable yes votes and recoverable effects.
- No transaction votes yes while an application predecessor remains unresolved;
  no committed descendant of an aborted transaction.
- No new execution or lock acquisition after global execution closure; no
  tentative effects visible without dependency capture.
- A pending acknowledgement is never counted as a durable vote. Duplicate and
  stale messages cannot revive a rejected attempt or change its manifest.
- Crash at every boundary: before/after stage append, lock release, stage flush,
  predecessor observation, yes-vote append/flush, decision append/flush, finalization,
  and snapshot creation.
- Reused sequence numbers and stale writer messages cannot satisfy old references.
- Partial staging plus a wound, an ancestor abort with descendants on several
  databases, and coordinator failure after all yes votes recover consistently.
- Protocol records beyond unresolved application work can still persist and
  resolve it; speculative rollback never erases unrelated durable decisions.
- No client/subscription success leaks from an aborted speculative attempt, and
  no state-dependent application error is exposed based on an aborted predecessor.

Measure execution-slot hold time separately from end-to-end commit latency,
throughput, speculative bytes/depth, durability batches, dependency wait time,
abort fanout, and recovery duration. Under reachable authorities and successful
storage, dependency chains should drain; partitions can force bounded admission
to stop without violating safety.

## Related work: Transactions for Distributed Actors in the Cloud

Tamer Eldeeb and Philip A. Bernstein's October 2016 paper,
[Transactions for Distributed Actors in the Cloud](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/10/EldeebBernstein-TransactionalActors-MSR-TR-1.pdf),
describes releasing locks during prepare, retaining speculative versions, tracking
commit dependencies, and cascading aborts. Sections II and III explain how this
overlaps execution with persistence and permits batching.

Their architecture uses a centralized transaction manager to validate dependencies
and record decisions. Section III.F also allows dependent transactions into the
same ordered commit queue before predecessors finish flushing: that shared log
preserves their durability order. Sections III.G–I address checkpointing, decision
retention, and consistent snapshot reads.

Our proposed adaptation uses per-database logs and per-transaction coordinators.
The baseline chooses to wait for durable predecessor decisions before a participant
votes yes; it cannot assume a shared decision log orders commits across
coordinators. Moving that check to the coordinator is one alternative described
above, but still does not supply the paper's shared decision ordering. The baseline
also uses database-wide dependencies and suffix reconstruction; the representation
of speculative versions remains open.

The paper establishes precedent for early lock release, not a proof of this
adaptation. Its centralized ordering and recovery mechanisms identify obligations
we must replace explicitly. In particular, distributed decision retention and
cross-database snapshots remain separate design work here.

## Open decisions

- Does the conservative protocol improve completed throughput enough to justify
  its costs, or would useful gains require different dependency or decision ordering?
- Which existing commitlog/oplog identity and fencing guarantees can implement
  `LogRef`, and how do they distinguish surviving records from replaced suffixes?
- Which datastore representation provides immutable staged effects and a committed
  view without prohibitive copying, including indexes, sequences, and schema state?
- Can stage and yes-vote records be combined on the common path while retaining
  recoverable dependency evidence and serialized rejection semantics?
- What bounds provide useful throughput without making cascading recovery too
  expensive, and how is staging capacity reserved across dynamically found participants?
- Which snapshot/retention mechanism safely reclaims decisions and dependency
  evidence? Unbounded retention is only a prototype expedient.
- What client/read guarantees would permit acknowledging at the durable decision
  before all participants finalize, and how would a later read enforce them? What
  retry and error behavior should clients see after a speculative dependency fails?

There are two linked feasibility questions: whether the overlap produces a useful
throughput gain, and whether speculative state, visibility, and recovery can be
provided at acceptable cost. The baseline illustrates how durability could be
removed from execution lock holding without continued execution after release or
a global coordination service. Whether that is the right tradeoff remains to be
established by the experiments.
