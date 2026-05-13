package com.clockworklabs.spacetimedb

interface Table<TRow> {
    val tableName: String
    val count: Int
    fun iter(): Sequence<TRow>
    fun onInsert(callback: (EventContext<*>, TRow) -> Unit): CallbackId
    fun removeOnInsert(id: CallbackId)
    fun onDelete(callback: (EventContext<*>, TRow) -> Unit): CallbackId
    fun removeOnDelete(id: CallbackId)
}

interface TableWithPrimaryKey<TRow> : Table<TRow> {
    fun onUpdate(callback: (EventContext<*>, TRow, TRow) -> Unit): CallbackId
    fun removeOnUpdate(id: CallbackId)
}

interface EventTable<TRow> {
    val tableName: String
    fun onInsert(callback: (EventContext<*>, TRow) -> Unit): CallbackId
    fun removeOnInsert(id: CallbackId)
}
