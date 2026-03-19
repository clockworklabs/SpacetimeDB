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
    public fun onInsert(cb: (EventContext, Row) -> Unit)
    public fun removeOnInsert(cb: (EventContext, Row) -> Unit)
}

/**
 * A generated table handle backed by a persistent (stored) table.
 * Provides read access to cached rows and callbacks for inserts, deletes, and before-delete.
 */
public interface RemotePersistentTable<Row> : RemoteTable<Row> {
    public fun count(): Int
    public fun all(): List<Row>
    public fun iter(): Sequence<Row>

    public fun onDelete(cb: (EventContext, Row) -> Unit)
    public fun removeOnDelete(cb: (EventContext, Row) -> Unit)
    public fun onBeforeDelete(cb: (EventContext, Row) -> Unit)
    public fun removeOnBeforeDelete(cb: (EventContext, Row) -> Unit)
}

/**
 * A [RemotePersistentTable] whose rows have a primary key.
 * Adds [onUpdate] / [removeOnUpdate] callbacks that fire when an existing row is replaced.
 */
public interface RemotePersistentTableWithPrimaryKey<Row> : RemotePersistentTable<Row> {
    public fun onUpdate(cb: (EventContext, Row, Row) -> Unit)
    public fun removeOnUpdate(cb: (EventContext, Row, Row) -> Unit)
}

/**
 * A generated table handle backed by an event (non-stored) table.
 * Rows are not cached; only insert callbacks fire per event.
 */
public interface RemoteEventTable<Row> : RemoteTable<Row>
