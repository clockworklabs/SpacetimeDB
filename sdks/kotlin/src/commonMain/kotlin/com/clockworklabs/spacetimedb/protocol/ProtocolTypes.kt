package com.clockworklabs.spacetimedb.protocol

import com.clockworklabs.spacetimedb.bsatn.BsatnReader
import com.clockworklabs.spacetimedb.bsatn.BsatnRowList
import com.clockworklabs.spacetimedb.bsatn.BsatnWriter

@kotlin.jvm.JvmInline
value class QuerySetId(val id: UInt) {
    companion object {
        fun read(reader: BsatnReader): QuerySetId = QuerySetId(reader.readU32())
        fun write(writer: BsatnWriter, value: QuerySetId) { writer.writeU32(value.id) }
    }
}

@kotlin.jvm.JvmInline
value class RawIdentifier(val value: String) {
    companion object {
        fun read(reader: BsatnReader): RawIdentifier = RawIdentifier(reader.readString())
        fun write(writer: BsatnWriter, value: RawIdentifier) { writer.writeString(value.value) }
    }
}

data class SingleTableRows(
    val table: RawIdentifier,
    val rows: BsatnRowList,
) {
    companion object {
        fun read(reader: BsatnReader): SingleTableRows = SingleTableRows(
            table = RawIdentifier.read(reader),
            rows = BsatnRowList.read(reader),
        )
    }
}

data class QueryRows(val tables: List<SingleTableRows>) {
    companion object {
        fun read(reader: BsatnReader): QueryRows =
            QueryRows(reader.readArray { SingleTableRows.read(it) })
    }
}

sealed class TableUpdateRows {
    data class PersistentTable(val rows: PersistentTableRows) : TableUpdateRows()
    data class EventTable(val rows: EventTableRows) : TableUpdateRows()

    companion object {
        fun read(reader: BsatnReader): TableUpdateRows {
            return when (reader.readTag().toInt()) {
                0 -> PersistentTable(PersistentTableRows.read(reader))
                1 -> EventTable(EventTableRows.read(reader))
                else -> throw IllegalStateException("Invalid TableUpdateRows tag")
            }
        }
    }
}

data class PersistentTableRows(
    val inserts: BsatnRowList,
    val deletes: BsatnRowList,
) {
    companion object {
        fun read(reader: BsatnReader): PersistentTableRows = PersistentTableRows(
            inserts = BsatnRowList.read(reader),
            deletes = BsatnRowList.read(reader),
        )
    }
}

data class EventTableRows(val events: BsatnRowList) {
    companion object {
        fun read(reader: BsatnReader): EventTableRows =
            EventTableRows(BsatnRowList.read(reader))
    }
}

data class TableUpdate(
    val tableName: RawIdentifier,
    val rows: List<TableUpdateRows>,
) {
    companion object {
        fun read(reader: BsatnReader): TableUpdate = TableUpdate(
            tableName = RawIdentifier.read(reader),
            rows = reader.readArray { TableUpdateRows.read(it) },
        )
    }
}

data class QuerySetUpdate(
    val querySetId: QuerySetId,
    val tables: List<TableUpdate>,
) {
    companion object {
        fun read(reader: BsatnReader): QuerySetUpdate = QuerySetUpdate(
            querySetId = QuerySetId.read(reader),
            tables = reader.readArray { TableUpdate.read(it) },
        )
    }
}

sealed class ReducerOutcome {
    data class Ok(val retValue: ByteArray, val transactionUpdate: TransactionUpdateData) : ReducerOutcome() {
        override fun equals(other: Any?): Boolean =
            other is Ok && retValue.contentEquals(other.retValue) && transactionUpdate == other.transactionUpdate
        override fun hashCode(): Int = retValue.contentHashCode() * 31 + transactionUpdate.hashCode()
    }
    data object OkEmpty : ReducerOutcome()
    data class Err(val message: ByteArray) : ReducerOutcome() {
        override fun equals(other: Any?): Boolean = other is Err && message.contentEquals(other.message)
        override fun hashCode(): Int = message.contentHashCode()
    }
    data class InternalError(val message: String) : ReducerOutcome()

    companion object {
        fun read(reader: BsatnReader): ReducerOutcome {
            return when (reader.readTag().toInt()) {
                0 -> Ok(
                    retValue = reader.readByteArray(),
                    transactionUpdate = TransactionUpdateData.read(reader),
                )
                1 -> OkEmpty
                2 -> Err(reader.readByteArray())
                3 -> InternalError(reader.readString())
                else -> throw IllegalStateException("Invalid ReducerOutcome tag")
            }
        }
    }
}

data class TransactionUpdateData(val querySets: List<QuerySetUpdate>) {
    companion object {
        fun read(reader: BsatnReader): TransactionUpdateData =
            TransactionUpdateData(reader.readArray { QuerySetUpdate.read(it) })
    }
}

sealed class ProcedureStatus {
    data class Returned(val data: ByteArray) : ProcedureStatus() {
        override fun equals(other: Any?): Boolean = other is Returned && data.contentEquals(other.data)
        override fun hashCode(): Int = data.contentHashCode()
    }
    data class InternalError(val message: String) : ProcedureStatus()

    companion object {
        fun read(reader: BsatnReader): ProcedureStatus {
            return when (reader.readTag().toInt()) {
                0 -> Returned(reader.readByteArray())
                1 -> InternalError(reader.readString())
                else -> throw IllegalStateException("Invalid ProcedureStatus tag")
            }
        }
    }
}

@kotlin.jvm.JvmInline
value class TimeDuration(val microseconds: ULong) {
    companion object {
        fun read(reader: BsatnReader): TimeDuration = TimeDuration(reader.readU64())
    }
}
