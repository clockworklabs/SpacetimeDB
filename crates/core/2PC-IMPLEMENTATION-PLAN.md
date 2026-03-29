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

## Holding MutTxId: dedicated blocking thread

`MutTxId` is `!Send` (holds `SharedWriteGuard`). The participant must hold it across multiple CALL requests from the coordinator for serializable isolation. The solution: a **dedicated blocking thread per participant transaction** that holds the `MutTxId` for its entire lifetime. Async HTTP handlers communicate with this thread via channels. The `MutTxId` never crosses a thread boundary or touches an async context.

```
HTTP handler (async)          Blocking thread (sync, holds MutTxId)
---------------------         -------------------------------------
CALL request arrives    --->  receive on channel, execute reducer
                        <---  send result back on channel
return HTTP response

CALL request arrives    --->  execute next reducer (same MutTxId)
                        <---  send result
return HTTP response

END_CALLS arrives       --->  commit in-memory, release write lock
                              send PREPARE to durability, barrier up
                              wait for durability...
                        <---  send PREPARED
return HTTP response

COMMIT arrives          --->  send COMMIT to durability, barrier down
                              thread exits
```

On first CALL for a new 2PC transaction:
1. Spawn a blocking thread (`std::thread::spawn` or `tokio::task::spawn_blocking`)
2. Thread creates `MutTxId` (acquires write lock)
3. Thread blocks on a command channel (`mpsc::Receiver<TxCommand>`)
4. Store the command sender (`mpsc::Sender<TxCommand>`) in a session map keyed by session_id
5. Return session_id to coordinator along with the first CALL's result

Subsequent CALLs and END_CALLS look up the session_id, send commands on the channel. The blocking thread processes them sequentially on the same `MutTxId`.

The blocking thread also needs access to a WASM module instance to execute reducers. The instance must be taken from the pool on thread creation and returned on thread exit (after COMMIT or ABORT).

```rust
enum TxCommand {
    Call { reducer: String, args: Bytes, reply: oneshot::Sender<CallResult> },
    EndCalls { reply: oneshot::Sender<()> },
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

## Key files

- `crates/core/src/db/relational_db.rs` -- PersistenceBarrier, send_or_buffer_durability, finalize_prepare_commit/abort
- `crates/core/src/host/prepared_tx.rs` -- PreparedTxInfo, PreparedTransactions registry
- `crates/core/src/host/module_host.rs` -- prepare_reducer, commit_prepared, abort_prepared
- `crates/core/src/host/wasm_common/module_host_actor.rs` -- coordinator post-commit coordination
- `crates/core/src/host/instance_env.rs` -- call_reducer_on_db_2pc, prepared_participants tracking
- `crates/core/src/host/wasmtime/wasm_instance_env.rs` -- WASM host function
- `crates/client-api/src/routes/database.rs` -- HTTP endpoints (CALL, END_CALLS, COMMIT, ABORT, PREPARED notification)
- `crates/bindings-sys/src/lib.rs` -- FFI
- `crates/bindings/src/remote_reducer.rs` -- safe wrapper
