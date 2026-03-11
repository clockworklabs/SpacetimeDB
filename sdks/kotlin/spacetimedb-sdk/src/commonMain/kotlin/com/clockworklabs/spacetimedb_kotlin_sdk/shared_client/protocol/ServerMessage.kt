package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader

// --- RowSizeHint ---
// Sum type: tag 0 = FixedSize(U16), tag 1 = RowOffsets(Array<U64>)

public sealed interface RowSizeHint {
    public data class FixedSize(val size: UShort) : RowSizeHint
    public data class RowOffsets(val offsets: List<ULong>) : RowSizeHint

    public companion object {
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

// --- BsatnRowList ---

public class BsatnRowList(
    public val sizeHint: RowSizeHint,
    private val rowsData: ByteArray,
    private val rowsOffset: Int = 0,
    private val rowsLimit: Int = rowsData.size,
) {
    public val rowsSize: Int get() = rowsLimit - rowsOffset

    /** Creates a fresh [BsatnReader] over the row data. Safe to call multiple times. */
    public val rowsReader: BsatnReader get() = BsatnReader(rowsData, rowsOffset, rowsLimit)

    public companion object {
        public fun decode(reader: BsatnReader): BsatnRowList {
            val sizeHint = RowSizeHint.decode(reader)
            val len = reader.readU32().toInt()
            val data = reader.data
            val offset = reader.offset
            reader.skip(len)
            return BsatnRowList(sizeHint, data, offset, offset + len)
        }
    }
}

// --- SingleTableRows ---

public data class SingleTableRows(
    val table: String,
    val rows: BsatnRowList,
) {
    public companion object {
        public fun decode(reader: BsatnReader): SingleTableRows {
            val table = reader.readString()
            val rows = BsatnRowList.decode(reader)
            return SingleTableRows(table, rows)
        }
    }
}

// --- QueryRows ---

public data class QueryRows(
    val tables: List<SingleTableRows>,
) {
    public companion object {
        public fun decode(reader: BsatnReader): QueryRows {
            val len = reader.readArrayLen()
            val tables = List(len) { SingleTableRows.decode(reader) }
            return QueryRows(tables)
        }
    }
}

// --- QueryResult ---

public sealed interface QueryResult {
    public data class Ok(val rows: QueryRows) : QueryResult
    public data class Err(val error: String) : QueryResult
}

// --- TableUpdateRows ---
// Sum type: tag 0 = PersistentTable(inserts, deletes), tag 1 = EventTable(events)

public sealed interface TableUpdateRows {
    public data class PersistentTable(
        val inserts: BsatnRowList,
        val deletes: BsatnRowList,
    ) : TableUpdateRows

    public data class EventTable(
        val events: BsatnRowList,
    ) : TableUpdateRows

    public companion object {
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

// --- TableUpdate ---

public data class TableUpdate(
    val tableName: String,
    val rows: List<TableUpdateRows>,
) {
    public companion object {
        public fun decode(reader: BsatnReader): TableUpdate {
            val tableName = reader.readString()
            val len = reader.readArrayLen()
            val rows = List(len) { TableUpdateRows.decode(reader) }
            return TableUpdate(tableName, rows)
        }
    }
}

// --- QuerySetUpdate ---

public data class QuerySetUpdate(
    val querySetId: QuerySetId,
    val tables: List<TableUpdate>,
) {
    public companion object {
        public fun decode(reader: BsatnReader): QuerySetUpdate {
            val querySetId = QuerySetId(reader.readU32())
            val len = reader.readArrayLen()
            val tables = List(len) { TableUpdate.decode(reader) }
            return QuerySetUpdate(querySetId, tables)
        }
    }
}

// --- TransactionUpdate ---

public data class TransactionUpdate(
    val querySets: List<QuerySetUpdate>,
) {
    public companion object {
        public fun decode(reader: BsatnReader): TransactionUpdate {
            val len = reader.readArrayLen()
            val querySets = List(len) { QuerySetUpdate.decode(reader) }
            return TransactionUpdate(querySets)
        }
    }
}

// --- ReducerOutcome ---
// Sum type: tag 0 = Ok(ReducerOk), tag 1 = OkEmpty, tag 2 = Err(ByteArray), tag 3 = InternalError(String)

public sealed interface ReducerOutcome {
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

    public data object OkEmpty : ReducerOutcome

    public data class Err(val error: ByteArray) : ReducerOutcome {
        override fun equals(other: Any?): Boolean =
            other is Err && error.contentEquals(other.error)

        override fun hashCode(): Int = error.contentHashCode()
    }

    public data class InternalError(val message: String) : ReducerOutcome

    public companion object {
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

// --- ProcedureStatus ---
// Sum type: tag 0 = Returned(ByteArray), tag 1 = InternalError(String)

public sealed interface ProcedureStatus {
    public data class Returned(val value: ByteArray) : ProcedureStatus {
        override fun equals(other: Any?): Boolean =
            other is Returned && value.contentEquals(other.value)

        override fun hashCode(): Int = value.contentHashCode()
    }

    public data class InternalError(val message: String) : ProcedureStatus

    public companion object {
        public fun decode(reader: BsatnReader): ProcedureStatus {
            return when (val tag = reader.readSumTag().toInt()) {
                0 -> Returned(reader.readByteArray())
                1 -> InternalError(reader.readString())
                else -> error("Unknown ProcedureStatus tag: $tag")
            }
        }
    }
}

// --- ServerMessage ---
// Sum type matching TS SDK's ServerMessage enum variants in order:
//   tag 0 = InitialConnection
//   tag 1 = SubscribeApplied
//   tag 2 = UnsubscribeApplied
//   tag 3 = SubscriptionError
//   tag 4 = TransactionUpdate
//   tag 5 = OneOffQueryResult
//   tag 6 = ReducerResult
//   tag 7 = ProcedureResult

public sealed interface ServerMessage {

    public data class InitialConnection(
        val identity: Identity,
        val connectionId: ConnectionId,
        val token: String,
    ) : ServerMessage

    public data class SubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows,
    ) : ServerMessage

    public data class UnsubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows?,
    ) : ServerMessage

    public data class SubscriptionError(
        val requestId: UInt?,
        val querySetId: QuerySetId,
        val error: String,
    ) : ServerMessage

    public data class TransactionUpdateMsg(
        val update: TransactionUpdate,
    ) : ServerMessage

    public data class OneOffQueryResult(
        val requestId: UInt,
        val result: QueryResult,
    ) : ServerMessage

    public data class ReducerResultMsg(
        val requestId: UInt,
        val timestamp: Timestamp,
        val result: ReducerOutcome,
    ) : ServerMessage

    public data class ProcedureResultMsg(
        val status: ProcedureStatus,
        val timestamp: Timestamp,
        val totalHostExecutionDuration: TimeDuration,
        val requestId: UInt,
    ) : ServerMessage

    public companion object {
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

        public fun decodeFromBytes(data: ByteArray): ServerMessage {
            val reader = BsatnReader(data)
            return decode(reader)
        }
    }
}
