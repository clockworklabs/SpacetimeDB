package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Sealed hierarchy for generated table handles.
 * Use `is RemotePersistentTable` / `is RemoteEventTable` to distinguish at runtime.
 *
 * - [RemotePersistentTable]: rows are stored in the client cache; supports
 *   count/all/iter, onDelete, and onBeforeDelete.
 * - [RemoteEventTable]: rows are NOT stored; only onInsert fires per event.
 */
public sealed interface RemoteTable<Row> {
    /** Registers a callback that fires when a row is inserted. */
    public fun onInsert(cb: (EventContext, Row) -> Unit)

    /** Removes a previously registered insert callback. */
    public fun removeOnInsert(cb: (EventContext, Row) -> Unit)
}

/**
 * A generated table handle backed by a persistent (stored) table.
 * Provides read access to cached rows and callbacks for inserts, deletes, and before-delete.
 */
public interface RemotePersistentTable<Row> : RemoteTable<Row> {
    /** Returns the number of rows currently in the client cache for this table. */
    public fun count(): Int

    /** Returns a snapshot list of all cached rows. */
    public fun all(): List<Row>

    /** Returns a lazy sequence over all cached rows. */
    public fun iter(): Sequence<Row>

    /** Registers a callback that fires after a row is deleted. */
    public fun onDelete(cb: (EventContext, Row) -> Unit)

    /** Removes a previously registered delete callback. */
    public fun removeOnDelete(cb: (EventContext, Row) -> Unit)

    /** Registers a callback that fires before a row is deleted. */
    public fun onBeforeDelete(cb: (EventContext, Row) -> Unit)

    /** Removes a previously registered before-delete callback. */
    public fun removeOnBeforeDelete(cb: (EventContext, Row) -> Unit)
}

/**
 * A [RemotePersistentTable] whose rows have a primary key.
 * Adds [onUpdate] / [removeOnUpdate] callbacks that fire when an existing row is replaced.
 */
public interface RemotePersistentTableWithPrimaryKey<Row> : RemotePersistentTable<Row> {
    /** Registers a callback that fires when an existing row is replaced (old row, new row). */
    public fun onUpdate(cb: (EventContext, Row, Row) -> Unit)

    /** Removes a previously registered update callback. */
    public fun removeOnUpdate(cb: (EventContext, Row, Row) -> Unit)
}

/**
 * A generated table handle backed by an event (non-stored) table.
 * Rows are not cached; only insert callbacks fire per event.
 */
public interface RemoteEventTable<Row> : RemoteTable<Row>
