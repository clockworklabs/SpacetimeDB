package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.InternalSpacetimeApi
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader

/** Hint describing how rows are packed in a [BsatnRowList]. */
@InternalSpacetimeApi
public sealed interface RowSizeHint {
    /** All rows have the same fixed byte size. */
    public data class FixedSize(val size: UShort) : RowSizeHint
    /** Variable-size rows; offsets indicate where each row ends. */
    public data class RowOffsets(val offsets: List<ULong>) : RowSizeHint

    public companion object {
        /** Decodes a [RowSizeHint] from BSATN. */
        public fun decode(reader: BsatnReader): RowSizeHint {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> FixedSize(reader.readU16())
                1 -> {
                    val len = reader.readArrayLen()
                    val offsets = List(len) { reader.readU64() }
                    RowOffsets(offsets)
                }
                else -> error("Unknown RowSizeHint tag: $tag")
            }
        }
    }
}

/** A BSATN-encoded list of rows with an associated [RowSizeHint]. */
@InternalSpacetimeApi
public class BsatnRowList(
    public val sizeHint: RowSizeHint,
    private val rowsData: ByteArray,
    private val rowsOffset: Int = 0,
    private val rowsLimit: Int = rowsData.size,
) {
    /** Total byte size of the row data. */
    public val rowsSize: Int get() = rowsLimit - rowsOffset

    /** Creates a fresh [BsatnReader] over the row data. Safe to call multiple times. */
    public val rowsReader: BsatnReader get() = BsatnReader(rowsData, rowsOffset, rowsLimit)

    public companion object {
        /** Decodes a [BsatnRowList] from BSATN. */
        public fun decode(reader: BsatnReader): BsatnRowList {
            val sizeHint = RowSizeHint.decode(reader)
            val rawLen = reader.readU32()
            check(rawLen <= Int.MAX_VALUE.toUInt()) { "BsatnRowList length $rawLen exceeds maximum supported size" }
            val len = rawLen.toInt()
            val data = reader.data
            val offset = reader.offset
            reader.skip(len)
            return BsatnRowList(sizeHint, data, offset, offset + len)
        }
    }
}

/** Rows belonging to a single table, identified by name. */
@InternalSpacetimeApi
public data class SingleTableRows(
    val table: String,
    val rows: BsatnRowList,
) {
    public companion object {
        /** Decodes a [SingleTableRows] from BSATN. */
        public fun decode(reader: BsatnReader): SingleTableRows {
            val table = reader.readString()
            val rows = BsatnRowList.decode(reader)
            return SingleTableRows(table, rows)
        }
    }
}

/** Collection of rows grouped by table, returned from a query. */
@InternalSpacetimeApi
public data class QueryRows(
    val tables: List<SingleTableRows>,
) {
    public companion object {
        /** Decodes a [QueryRows] from BSATN. */
        public fun decode(reader: BsatnReader): QueryRows {
            val len = reader.readArrayLen()
            val tables = List(len) { SingleTableRows.decode(reader) }
            return QueryRows(tables)
        }
    }
}

/** Result of a query: either successful rows or an error message. */
@InternalSpacetimeApi
public sealed interface QueryResult {
    /** Successful query result containing the returned rows. */
    public data class Ok(val rows: QueryRows) : QueryResult
    /** Failed query result containing an error message. */
    public data class Err(val error: String) : QueryResult
}

/** Row updates for a single table within a transaction. */
@InternalSpacetimeApi
public sealed interface TableUpdateRows {
    /** Inserts and deletes for a persistent (stored) table. */
    public data class PersistentTable(
        val inserts: BsatnRowList,
        val deletes: BsatnRowList,
    ) : TableUpdateRows

    /** Events for an event (non-stored) table. */
    public data class EventTable(
        val events: BsatnRowList,
    ) : TableUpdateRows

    public companion object {
        /** Decodes a [TableUpdateRows] from BSATN. */
        public fun decode(reader: BsatnReader): TableUpdateRows {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> PersistentTable(
                    inserts = BsatnRowList.decode(reader),
                    deletes = BsatnRowList.decode(reader),
                )
                1 -> EventTable(events = BsatnRowList.decode(reader))
                else -> error("Unknown TableUpdateRows tag: $tag")
            }
        }
    }
}

/** Update for a single table: its name and the list of row changes. */
@InternalSpacetimeApi
public data class TableUpdate(
    val tableName: String,
    val rows: List<TableUpdateRows>,
) {
    public companion object {
        /** Decodes a [TableUpdate] from BSATN. */
        public fun decode(reader: BsatnReader): TableUpdate {
            val tableName = reader.readString()
            val len = reader.readArrayLen()
            val rows = List(len) { TableUpdateRows.decode(reader) }
            return TableUpdate(tableName, rows)
        }
    }
}

/** Table updates scoped to a single query set. */
@InternalSpacetimeApi
public data class QuerySetUpdate(
    val querySetId: QuerySetId,
    val tables: List<TableUpdate>,
) {
    public companion object {
        /** Decodes a [QuerySetUpdate] from BSATN. */
        public fun decode(reader: BsatnReader): QuerySetUpdate {
            val querySetId = QuerySetId(reader.readU32())
            val len = reader.readArrayLen()
            val tables = List(len) { TableUpdate.decode(reader) }
            return QuerySetUpdate(querySetId, tables)
        }
    }
}

/** A complete transaction update containing changes across all affected query sets. */
@InternalSpacetimeApi
public data class TransactionUpdate(
    val querySets: List<QuerySetUpdate>,
) {
    public companion object {
        /** Decodes a [TransactionUpdate] from BSATN. */
        public fun decode(reader: BsatnReader): TransactionUpdate {
            val len = reader.readArrayLen()
            val querySets = List(len) { QuerySetUpdate.decode(reader) }
            return TransactionUpdate(querySets)
        }
    }
}

/** Outcome of a reducer execution on the server. */
@InternalSpacetimeApi
public sealed interface ReducerOutcome {
    /** Reducer succeeded with a return value and transaction update. */
    public data class Ok(
        val retValue: ByteArray,
        val transactionUpdate: TransactionUpdate,
    ) : ReducerOutcome {
        override fun equals(other: Any?): Boolean =
            other is Ok &&
                retValue.contentEquals(other.retValue) &&
                transactionUpdate == other.transactionUpdate

        override fun hashCode(): Int {
            var result = retValue.contentHashCode()
            result = 31 * result + transactionUpdate.hashCode()
            return result
        }
    }

    /** Reducer succeeded with no return value and no table changes. */
    public data object OkEmpty : ReducerOutcome

    /** Reducer failed with a BSATN-encoded error. */
    public data class Err(val error: ByteArray) : ReducerOutcome {
        override fun equals(other: Any?): Boolean =
            other is Err && error.contentEquals(other.error)

        override fun hashCode(): Int = error.contentHashCode()
    }

    /** Reducer encountered an internal server error. */
    public data class InternalError(val message: String) : ReducerOutcome

    public companion object {
        /** Decodes a [ReducerOutcome] from BSATN. */
        public fun decode(reader: BsatnReader): ReducerOutcome {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> Ok(
                    retValue = reader.readByteArray(),
                    transactionUpdate = TransactionUpdate.decode(reader),
                )
                1 -> OkEmpty
                2 -> Err(reader.readByteArray())
                3 -> InternalError(reader.readString())
                else -> error("Unknown ReducerOutcome tag: $tag")
            }
        }
    }
}

/** Status of a procedure execution on the server. */
@InternalSpacetimeApi
public sealed interface ProcedureStatus {
    /** Procedure returned successfully with a BSATN-encoded value. */
    public data class Returned(val value: ByteArray) : ProcedureStatus {
        override fun equals(other: Any?): Boolean =
            other is Returned && value.contentEquals(other.value)

        override fun hashCode(): Int = value.contentHashCode()
    }

    /** Procedure encountered an internal server error. */
    public data class InternalError(val message: String) : ProcedureStatus

    public companion object {
        /** Decodes a [ProcedureStatus] from BSATN. */
        public fun decode(reader: BsatnReader): ProcedureStatus {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> Returned(reader.readByteArray())
                1 -> InternalError(reader.readString())
                else -> error("Unknown ProcedureStatus tag: $tag")
            }
        }
    }
}

/**
 * Messages received from the SpacetimeDB server.
 * Variant tags match the wire protocol (0=InitialConnection through 7=ProcedureResult).
 */
@InternalSpacetimeApi
public sealed interface ServerMessage {

    /** Server confirmed the connection and assigned identity/token. */
    public data class InitialConnection(
        val identity: Identity,
        val connectionId: ConnectionId,
        val token: String,
    ) : ServerMessage

    /** Server applied a subscription and returned the initial matching rows. */
    public data class SubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows,
    ) : ServerMessage

    /** Server confirmed an unsubscription, optionally returning dropped rows. */
    public data class UnsubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows?,
    ) : ServerMessage

    /** Server reported an error for a subscription. */
    public data class SubscriptionError(
        val requestId: UInt?,
        val querySetId: QuerySetId,
        val error: String,
    ) : ServerMessage

    /** A transaction update containing table changes from a server-side event. */
    public data class TransactionUpdateMsg(
        val update: TransactionUpdate,
    ) : ServerMessage

    /** Result of a one-off SQL query. */
    public data class OneOffQueryResult(
        val requestId: UInt,
        val result: QueryResult,
    ) : ServerMessage

    /** Result of a reducer call, including timestamp and outcome. */
    public data class ReducerResultMsg(
        val requestId: UInt,
        val timestamp: Timestamp,
        val result: ReducerOutcome,
    ) : ServerMessage

    /** Result of a procedure call, including status and execution duration. */
    public data class ProcedureResultMsg(
        val status: ProcedureStatus,
        val timestamp: Timestamp,
        val totalHostExecutionDuration: TimeDuration,
        val requestId: UInt,
    ) : ServerMessage

    public companion object {
        /** Decodes a [ServerMessage] from BSATN. */
        public fun decode(reader: BsatnReader): ServerMessage {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> InitialConnection(
                    identity = Identity.decode(reader),
                    connectionId = ConnectionId.decode(reader),
                    token = reader.readString(),
                )
                1 -> SubscribeApplied(
                    requestId = reader.readU32(),
                    querySetId = QuerySetId(reader.readU32()),
                    rows = QueryRows.decode(reader),
                )
                2 -> {
                    val requestId = reader.readU32()
                    val querySetId = QuerySetId(reader.readU32())
                    // Option<QueryRows>: tag 0 = Some, tag 1 = None
                    val rows = when (reader.readSumTag().toInt()) {
                        0 -> QueryRows.decode(reader)
                        1 -> null
                        else -> error("Invalid Option tag")
                    }
                    UnsubscribeApplied(requestId, querySetId, rows)
                }
                3 -> {
                    // Option<u32>: tag 0 = Some, tag 1 = None
                    val requestId = when (reader.readSumTag().toInt()) {
                        0 -> reader.readU32()
                        1 -> null
                        else -> error("Invalid Option tag")
                    }
                    val querySetId = QuerySetId(reader.readU32())
                    val error = reader.readString()
                    SubscriptionError(requestId, querySetId, error)
                }
                4 -> TransactionUpdateMsg(TransactionUpdate.decode(reader))
                5 -> {
                    val requestId = reader.readU32()
                    // Result<QueryRows, String>: tag 0 = Ok, tag 1 = Err
                    val result = when (reader.readSumTag().toInt()) {
                        0 -> QueryResult.Ok(QueryRows.decode(reader))
                        1 -> QueryResult.Err(reader.readString())
                        else -> error("Invalid Result tag")
                    }
                    OneOffQueryResult(requestId, result)
                }
                6 -> ReducerResultMsg(
                    requestId = reader.readU32(),
                    timestamp = Timestamp.decode(reader),
                    result = ReducerOutcome.decode(reader),
                )
                7 -> ProcedureResultMsg(
                    status = ProcedureStatus.decode(reader),
                    timestamp = Timestamp.decode(reader),
                    totalHostExecutionDuration = TimeDuration.decode(reader),
                    requestId = reader.readU32(),
                )
                else -> error("Unknown ServerMessage tag: $tag")
            }
        }

        /** Decodes a [ServerMessage] from a raw byte array. */
        public fun decodeFromBytes(data: ByteArray, offset: Int = 0): ServerMessage {
            val reader = BsatnReader(data, offset = offset)
            return decode(reader)
        }
    }
}
