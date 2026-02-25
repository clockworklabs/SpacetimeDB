@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader

// --- RowSizeHint ---
// Sum type: tag 0 = FixedSize(U16), tag 1 = RowOffsets(Array<U64>)

sealed interface RowSizeHint {
    data class FixedSize(val size: UShort) : RowSizeHint
    data class RowOffsets(val offsets: List<ULong>) : RowSizeHint

    companion object {
        fun decode(reader: BsatnReader): RowSizeHint {
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

data class BsatnRowList(
    val sizeHint: RowSizeHint,
    val rowsReader: BsatnReader,
) {
    val rowsSize: Int get() = rowsReader.remaining

    companion object {
        fun decode(reader: BsatnReader): BsatnRowList {
            val sizeHint = RowSizeHint.decode(reader)
            val len = reader.readU32().toInt()
            val rowsReader = reader.readRawBytesView(len)
            return BsatnRowList(sizeHint, rowsReader)
        }
    }
}

// --- SingleTableRows ---

data class SingleTableRows(
    val table: String,
    val rows: BsatnRowList,
) {
    companion object {
        fun decode(reader: BsatnReader): SingleTableRows {
            val table = reader.readString()
            val rows = BsatnRowList.decode(reader)
            return SingleTableRows(table, rows)
        }
    }
}

// --- QueryRows ---

data class QueryRows(
    val tables: List<SingleTableRows>,
) {
    companion object {
        fun decode(reader: BsatnReader): QueryRows {
            val len = reader.readArrayLen()
            val tables = List(len) { SingleTableRows.decode(reader) }
            return QueryRows(tables)
        }
    }
}

// --- QueryResult ---

sealed interface QueryResult {
    data class Ok(val rows: QueryRows) : QueryResult
    data class Err(val error: String) : QueryResult
}

// --- TableUpdateRows ---
// Sum type: tag 0 = PersistentTable(inserts, deletes), tag 1 = EventTable(events)

sealed interface TableUpdateRows {
    data class PersistentTable(
        val inserts: BsatnRowList,
        val deletes: BsatnRowList,
    ) : TableUpdateRows

    data class EventTable(
        val events: BsatnRowList,
    ) : TableUpdateRows

    companion object {
        fun decode(reader: BsatnReader): TableUpdateRows {
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

data class TableUpdate(
    val tableName: String,
    val rows: List<TableUpdateRows>,
) {
    companion object {
        fun decode(reader: BsatnReader): TableUpdate {
            val tableName = reader.readString()
            val len = reader.readArrayLen()
            val rows = List(len) { TableUpdateRows.decode(reader) }
            return TableUpdate(tableName, rows)
        }
    }
}

// --- QuerySetUpdate ---

data class QuerySetUpdate(
    val querySetId: QuerySetId,
    val tables: List<TableUpdate>,
) {
    companion object {
        fun decode(reader: BsatnReader): QuerySetUpdate {
            val querySetId = QuerySetId(reader.readU32())
            val len = reader.readArrayLen()
            val tables = List(len) { TableUpdate.decode(reader) }
            return QuerySetUpdate(querySetId, tables)
        }
    }
}

// --- TransactionUpdate ---

data class TransactionUpdate(
    val querySets: List<QuerySetUpdate>,
) {
    companion object {
        fun decode(reader: BsatnReader): TransactionUpdate {
            val len = reader.readArrayLen()
            val querySets = List(len) { QuerySetUpdate.decode(reader) }
            return TransactionUpdate(querySets)
        }
    }
}

// --- ReducerOutcome ---
// Sum type: tag 0 = Ok(ReducerOk), tag 1 = OkEmpty, tag 2 = Err(ByteArray), tag 3 = InternalError(String)

sealed interface ReducerOutcome {
    data class Ok(
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

    data object OkEmpty : ReducerOutcome

    data class Err(val error: ByteArray) : ReducerOutcome {
        override fun equals(other: Any?): Boolean =
            other is Err && error.contentEquals(other.error)

        override fun hashCode(): Int = error.contentHashCode()
    }

    data class InternalError(val message: String) : ReducerOutcome

    companion object {
        fun decode(reader: BsatnReader): ReducerOutcome {
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

sealed interface ProcedureStatus {
    data class Returned(val value: ByteArray) : ProcedureStatus {
        override fun equals(other: Any?): Boolean =
            other is Returned && value.contentEquals(other.value)

        override fun hashCode(): Int = value.contentHashCode()
    }

    data class InternalError(val message: String) : ProcedureStatus

    companion object {
        fun decode(reader: BsatnReader): ProcedureStatus {
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

sealed interface ServerMessage {

    data class InitialConnection(
        val identity: Identity,
        val connectionId: ConnectionId,
        val token: String,
    ) : ServerMessage

    data class SubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows,
    ) : ServerMessage

    data class UnsubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows?,
    ) : ServerMessage

    data class SubscriptionError(
        val requestId: UInt?,
        val querySetId: QuerySetId,
        val error: String,
    ) : ServerMessage

    data class TransactionUpdateMsg(
        val update: TransactionUpdate,
    ) : ServerMessage

    data class OneOffQueryResult(
        val requestId: UInt,
        val result: QueryResult,
    ) : ServerMessage

    data class ReducerResultMsg(
        val requestId: UInt,
        val timestamp: Timestamp,
        val result: ReducerOutcome,
    ) : ServerMessage

    data class ProcedureResultMsg(
        val status: ProcedureStatus,
        val timestamp: Timestamp,
        val totalHostExecutionDuration: TimeDuration,
        val requestId: UInt,
    ) : ServerMessage

    companion object {
        fun decode(reader: BsatnReader): ServerMessage {
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

        fun decodeFromBytes(data: ByteArray): ServerMessage {
            val reader = BsatnReader(data)
            return decode(reader)
        }
    }
}
