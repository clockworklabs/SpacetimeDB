# 2PC Implementation Plan (Pipelined)

## Context

The TPC-C benchmark on branch `origin/phoebe/tpcc/reducer-return-value` (public submodule) uses non-atomic HTTP calls for cross-database operations. We need 2PC so distributed transactions either commit on both databases or neither. Pipelined 2PC is chosen because it avoids blocking on persistence during lock-holding, and the codebase already separates in-memory commit from durability.

## Protocol

### Participant happy path:

1. Receive CALL from coordinator (reducer name + args)
2. Execute reducer (write lock held)
3. Return result to coordinator (write lock still held, transaction still open)
4. Possibly receive more CALLs from coordinator (same transaction, same write lock)
5. Receive END_CALLS from coordinator ("no more reducer calls in this transaction")
6. Commit in-memory (release write lock)
7. Send PREPARE to durability worker
8. **Barrier up** -- no more durability requests go through
9. In background: wait for PREPARE to be durable
10. Once durable: send PREPARED to coordinator
11. Wait for COMMIT or ABORT from coordinator
12. Receive COMMIT
13. Send COMMIT to durability worker
14. **Barrier down** -- flush buffered requests

### Coordinator happy path:

1. Execute reducer, calling participant reducers along the way (participants hold write locks, return results, but don't commit)
2. Reducer succeeds
3. Send END_CALLS to all participants (they can now commit in-memory)
4. Commit coordinator in-memory (release write lock)
5. Send PREPARE to durability worker
6. **Barrier up** -- no more durability requests go through
7. Wait for coordinator's own PREPARE to be durable
8. Wait for all participants to report PREPARED
9. Send COMMIT to all participants
10. Send COMMIT to durability worker
11. **Barrier down** -- flush buffered requests

## Key correctness properties

- **Serializable isolation**: Participant holds write lock from CALL through END_CALLS. Multiple CALLs from the same coordinator transaction execute within the same MutTxId on the participant. The second call sees the first call's writes.
- **Persistence barrier**: After PREPARE is sent to durability (step 7/8 on participant, step 5/6 on coordinator), no speculative transactions can reach the durability worker until COMMIT or ABORT. Anything sent to the durability worker can eventually become persistent, so the barrier is required.
- **Two responses from participant**: The immediate result (step 3) and the later PREPARED notification (step 10). The coordinator collects both: results during reducer execution, PREPARED notifications before deciding COMMIT.
- **Pipelining benefit**: Locks are held only during reducer execution (steps 1-6), not during persistence (steps 7-14). The persistence and 2PC handshake happen after locks are released on both sides.

## Holding MutTxId: reuse existing blocking pattern

`MutTxId` is `!Send` (holds `SharedWriteGuard`). The participant must hold it across multiple CALL requests from the coordinator for serializable isolation.

The codebase already has a blocking pattern: on the coordinator side, `call_reducer_on_db` uses `std::thread::scope` + `Handle::block_on` to block the WASM thread while making an async HTTP call. The same pattern works for the participant: instead of returning from the reducer execution, the participant's thread blocks on a channel (`blocking_recv`) waiting for the next command. The `MutTxId` stays alive on that same thread. No new threading model is needed.

```
Coordinator thread                 Participant thread
(WASM reducer running,             (holds MutTxId, holds WASM instance)
 holds coordinator MutTxId)

call_reducer_on_db_2pc()
  |
  |-- HTTP POST /2pc/begin/debit -> spawn thread, create MutTxId
  |                                 execute reducer
  |                                 send result via channel
  |   <-- HTTP response (result     block on channel (blocking_recv)
  |        + session_id)              |
  |                                   |   [MutTxId held, write lock held]
  |                                   |
call_reducer_on_db_2pc() (2nd call)   |
  |                                   |
  |-- HTTP POST /2pc/{sid}/call/x -> send command via channel
  |                                  wake up, execute reducer
  |                                  send result via channel
  |   <-- HTTP response              block on channel
  |                                   |
reducer finishes                      |
  |                                   |
[post-commit coordination]            |
  |                                   |
  |-- HTTP POST /2pc/{sid}/end  ---> wake up, commit in-memory
  |                                  release write lock
  |                                  send PREPARE to durability
  |                                  barrier up
  |                                  wait for PREPARE durable...
  |   <-- HTTP response (PREPARED)   block on channel
  |                                   |
  |-- HTTP POST /2pc/{sid}/commit -> wake up
  |                                  send COMMIT to durability
  |                                  barrier down, flush
  |   <-- HTTP response              thread exits
```

### Implementation

On first CALL for a new 2PC transaction:
1. The async HTTP handler spawns a blocking thread (via `std::thread::scope` or `tokio::task::spawn_blocking`)
2. The blocking thread takes a WASM instance from the module's instance pool
3. The blocking thread creates `MutTxId` (acquires write lock)
4. The blocking thread executes the first reducer
5. The blocking thread sends the result back via a `oneshot` channel
6. The async HTTP handler receives the result and returns the HTTP response with a `session_id`
7. The blocking thread blocks on a `mpsc::Receiver<TxCommand>` waiting for the next command
8. The async HTTP handler stores the `mpsc::Sender<TxCommand>` in a session map keyed by `session_id`

Subsequent CALLs and END_CALLS look up the `session_id`, send commands on the channel. The blocking thread processes them sequentially on the same `MutTxId`.

When the thread exits (after COMMIT or ABORT), it returns the WASM instance to the pool.

```rust
enum TxCommand {
    Call { reducer: String, args: Bytes, reply: oneshot::Sender<CallResult> },
    EndCalls { reply: oneshot::Sender<PreparedResult> },
    Commit { reply: oneshot::Sender<()> },
    Abort { reply: oneshot::Sender<()> },
}
```

## Abort paths

**Coordinator's reducer fails (step 2):**
- Send ABORT to all participants (they still hold write locks)
- Participants rollback their MutTxId (release write lock, no changes)
- No PREPARE was sent, no barrier needed

**Participant's reducer fails (step 2):**
- Participant returns error to coordinator
- Coordinator's reducer fails (propagates error)
- Coordinator sends ABORT to all other participants that succeeded
- Those participants rollback their MutTxId

**Coordinator's PREPARE persists but a participant's PREPARE fails to persist:**
- Participant cannot send PREPARED
- Coordinator times out waiting for PREPARED
- Coordinator sends ABORT to all participants
- Coordinator inverts its own in-memory state, discards buffered durability requests

**Crash during protocol:**
- See proposal in `proposals/00XX-inter-database-communication.md` section 8 for recovery rules

## Commitlog format

- PREPARE record: includes all row changes (inserts/deletes)
- COMMIT record: follows PREPARE, marks transaction as committed
- ABORT record: follows PREPARE, marks transaction as aborted
- No other records can appear between PREPARE and COMMIT/ABORT in the durable log (persistence barrier enforces this)

## Replay semantics

On replay, when encountering a PREPARE:
- Do not apply it to the datastore
- Read the next record:
  - COMMIT: apply the PREPARE's changes
  - ABORT: skip the PREPARE
  - No next record (crash): transaction is still in progress, wait for coordinator or timeout and abort

## Persistence barrier

The barrier in `relational_db.rs` has three states: `Inactive`, `Armed`, `Active`.

- **Inactive**: normal operation, durability requests go through.
- **Armed**: set BEFORE committing the transaction (while write lock is held). The NEXT durability request (the PREPARE) goes through to the worker and transitions the barrier to Active.
- **Active**: all subsequent durability requests are buffered.

This ensures no race between the write lock release and the barrier activation. Since the barrier is Armed while the write lock is held, no other transaction can commit and send a durability request before the barrier transitions to Active.

Used by both coordinator and participant:
- Arm before committing the 2PC transaction
- The commit's durability request (the PREPARE) transitions Armed -> Active
- On COMMIT: deactivate, flush buffered requests
- On ABORT: deactivate, discard buffered requests

## Key files

- `crates/core/src/db/relational_db.rs` -- PersistenceBarrier (Inactive/Armed/Active), send_or_buffer_durability, finalize_prepare_commit/abort
- `crates/core/src/host/prepared_tx.rs` -- TxCommand, TxSession, PreparedTransactions registry, session map
- `crates/core/src/host/module_host.rs` -- begin_2pc_session, commit_prepared, abort_prepared
- `crates/core/src/host/wasm_common/module_host_actor.rs` -- coordinator post-commit coordination (END_CALLS, wait PREPARED, COMMIT)
- `crates/core/src/host/instance_env.rs` -- call_reducer_on_db_2pc, prepared_participants tracking
- `crates/core/src/host/wasmtime/wasm_instance_env.rs` -- WASM host function
- `crates/client-api/src/routes/database.rs` -- HTTP endpoints: /2pc/begin/:reducer, /2pc/:sid/call/:reducer, /2pc/:sid/end, /2pc/:sid/commit, /2pc/:sid/abort
- `crates/bindings-sys/src/lib.rs` -- FFI
- `crates/bindings/src/remote_reducer.rs` -- safe wrapper
