package com.clockworklabs.spacetimedb.protocol

import com.clockworklabs.spacetimedb.ConnectionId
import com.clockworklabs.spacetimedb.Identity
import com.clockworklabs.spacetimedb.Timestamp
import com.clockworklabs.spacetimedb.bsatn.BsatnReader

sealed class ServerMessage {
    data class InitialConnection(
        val identity: Identity,
        val connectionId: ConnectionId,
        val token: String,
    ) : ServerMessage()

    data class SubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows,
    ) : ServerMessage()

    data class UnsubscribeApplied(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val rows: QueryRows?,
    ) : ServerMessage()

    data class SubscriptionError(
        val requestId: UInt?,
        val querySetId: QuerySetId,
        val error: String,
    ) : ServerMessage()

    data class TransactionUpdate(
        val querySets: List<QuerySetUpdate>,
    ) : ServerMessage()

    data class OneOffQueryResult(
        val requestId: UInt,
        val rows: QueryRows?,
        val error: String?,
    ) : ServerMessage()

    data class ReducerResult(
        val requestId: UInt,
        val timestamp: Timestamp,
        val result: ReducerOutcome,
    ) : ServerMessage()

    data class ProcedureResult(
        val requestId: UInt,
        val timestamp: Timestamp,
        val status: ProcedureStatus,
        val totalHostExecutionDuration: TimeDuration,
    ) : ServerMessage()

    companion object {
        fun decode(data: ByteArray): ServerMessage {
            val reader = BsatnReader(data)
            return when (reader.readTag().toInt()) {
                0 -> InitialConnection(
                    identity = Identity.read(reader),
                    connectionId = ConnectionId.read(reader),
                    token = reader.readString(),
                )
                1 -> SubscribeApplied(
                    requestId = reader.readU32(),
                    querySetId = QuerySetId.read(reader),
                    rows = QueryRows.read(reader),
                )
                2 -> UnsubscribeApplied(
                    requestId = reader.readU32(),
                    querySetId = QuerySetId.read(reader),
                    rows = reader.readOption { QueryRows.read(it) },
                )
                3 -> SubscriptionError(
                    requestId = reader.readOption { it.readU32() },
                    querySetId = QuerySetId.read(reader),
                    error = reader.readString(),
                )
                4 -> TransactionUpdate(
                    querySets = reader.readArray { QuerySetUpdate.read(it) },
                )
                5 -> {
                    val requestId = reader.readU32()
                    when (reader.readTag().toInt()) {
                        0 -> OneOffQueryResult(requestId, QueryRows.read(reader), null)
                        1 -> OneOffQueryResult(requestId, null, reader.readString())
                        else -> throw IllegalStateException("Invalid OneOffQueryResult Result tag")
                    }
                }
                6 -> ReducerResult(
                    requestId = reader.readU32(),
                    timestamp = Timestamp.read(reader),
                    result = ReducerOutcome.read(reader),
                )
                7 -> ProcedureResult(
                    status = ProcedureStatus.read(reader),
                    timestamp = Timestamp.read(reader),
                    totalHostExecutionDuration = TimeDuration.read(reader),
                    requestId = reader.readU32(),
                )
                else -> throw IllegalStateException("Unknown ServerMessage tag")
            }
        }

    }
}
