package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.time.Duration

/** Encode→decode round-trip tests for ClientMessage and ServerMessage. */
class ProtocolRoundTripTest {

    // ---- ClientMessage round-trips (encode → decode → assertEquals) ----

    @Test
    fun `client message subscribe round trip`() {
        val original = ClientMessage.Subscribe(
            requestId = 42u,
            querySetId = QuerySetId(7u),
            queryStrings = listOf("SELECT * FROM player", "SELECT * FROM item WHERE owner = 1"),
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message subscribe empty queries round trip`() {
        val original = ClientMessage.Subscribe(
            requestId = 0u,
            querySetId = QuerySetId(0u),
            queryStrings = emptyList(),
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message unsubscribe default round trip`() {
        val original = ClientMessage.Unsubscribe(
            requestId = 10u,
            querySetId = QuerySetId(3u),
            flags = UnsubscribeFlags.Default,
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message unsubscribe send dropped rows round trip`() {
        val original = ClientMessage.Unsubscribe(
            requestId = 10u,
            querySetId = QuerySetId(3u),
            flags = UnsubscribeFlags.SendDroppedRows,
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message one off query round trip`() {
        val original = ClientMessage.OneOffQuery(
            requestId = 99u,
            queryString = "SELECT count(*) FROM users",
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message call reducer round trip`() {
        val original = ClientMessage.CallReducer(
            requestId = 5u,
            flags = 0u,
            reducer = "add_player",
            args = byteArrayOf(1, 2, 3, 4, 5),
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message call reducer empty args round trip`() {
        val original = ClientMessage.CallReducer(
            requestId = 0u,
            flags = 1u,
            reducer = "noop",
            args = byteArrayOf(),
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    @Test
    fun `client message call procedure round trip`() {
        val original = ClientMessage.CallProcedure(
            requestId = 77u,
            flags = 0u,
            procedure = "get_leaderboard",
            args = byteArrayOf(10, 20),
        )
        val decoded = roundTripClientMessage(original)
        assertEquals(original, decoded)
    }

    // ---- ServerMessage round-trips (encode → decode → re-encode → assertContentEquals) ----
    // ServerMessage types containing BsatnRowList don't have value equality,
    // so we verify encode→decode→re-encode produces identical bytes.

    @Test
    fun `server message initial connection round trip`() {
        val original = ServerMessage.InitialConnection(
            identity = Identity(BigInteger.parseString("123456789ABCDEF", 16)),
            connectionId = ConnectionId(BigInteger.parseString("FEDCBA987654321", 16)),
            token = "my-auth-token",
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message subscribe applied round trip`() {
        val original = ServerMessage.SubscribeApplied(
            requestId = 1u,
            querySetId = QuerySetId(5u),
            rows = QueryRows(listOf(
                SingleTableRows("player", buildRowList(SampleRow(1, "Alice").encode())),
            )),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message subscribe applied empty rows round trip`() {
        val original = ServerMessage.SubscribeApplied(
            requestId = 0u,
            querySetId = QuerySetId(0u),
            rows = QueryRows(emptyList()),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message unsubscribe applied with rows round trip`() {
        val original = ServerMessage.UnsubscribeApplied(
            requestId = 2u,
            querySetId = QuerySetId(3u),
            rows = QueryRows(listOf(
                SingleTableRows("item", buildRowList(SampleRow(42, "sword").encode())),
            )),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message unsubscribe applied null rows round trip`() {
        val original = ServerMessage.UnsubscribeApplied(
            requestId = 2u,
            querySetId = QuerySetId(3u),
            rows = null,
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message subscription error with request id round trip`() {
        val original = ServerMessage.SubscriptionError(
            requestId = 10u,
            querySetId = QuerySetId(4u),
            error = "table not found",
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message subscription error null request id round trip`() {
        val original = ServerMessage.SubscriptionError(
            requestId = null,
            querySetId = QuerySetId(4u),
            error = "fatal error",
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message transaction update round trip`() {
        val row1 = SampleRow(1, "Alice").encode()
        val row2 = SampleRow(2, "Bob").encode()
        val original = ServerMessage.TransactionUpdateMsg(
            TransactionUpdate(listOf(
                QuerySetUpdate(
                    QuerySetId(1u),
                    listOf(
                        TableUpdate("player", listOf(
                            TableUpdateRows.PersistentTable(
                                inserts = buildRowList(row2),
                                deletes = buildRowList(row1),
                            ),
                        )),
                    ),
                ),
            )),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message transaction update event table round trip`() {
        val row = SampleRow(1, "event_data").encode()
        val original = ServerMessage.TransactionUpdateMsg(
            TransactionUpdate(listOf(
                QuerySetUpdate(
                    QuerySetId(2u),
                    listOf(
                        TableUpdate("events", listOf(
                            TableUpdateRows.EventTable(events = buildRowList(row)),
                        )),
                    ),
                ),
            )),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message one off query result ok round trip`() {
        val original = ServerMessage.OneOffQueryResult(
            requestId = 55u,
            result = QueryResult.Ok(QueryRows(listOf(
                SingleTableRows("users", buildRowList(SampleRow(1, "test").encode())),
            ))),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message one off query result err round trip`() {
        val original = ServerMessage.OneOffQueryResult(
            requestId = 55u,
            result = QueryResult.Err("syntax error near 'SELEC'"),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message reducer result ok round trip`() {
        val original = ServerMessage.ReducerResultMsg(
            requestId = 8u,
            timestamp = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L),
            result = ReducerOutcome.Ok(
                retValue = byteArrayOf(42),
                transactionUpdate = TransactionUpdate(emptyList()),
            ),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message reducer result ok empty round trip`() {
        val original = ServerMessage.ReducerResultMsg(
            requestId = 9u,
            timestamp = Timestamp.UNIX_EPOCH,
            result = ReducerOutcome.OkEmpty,
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message reducer result err round trip`() {
        val original = ServerMessage.ReducerResultMsg(
            requestId = 10u,
            timestamp = Timestamp.UNIX_EPOCH,
            result = ReducerOutcome.Err(byteArrayOf(0xDE.toByte(), 0xAD.toByte())),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message reducer result internal error round trip`() {
        val original = ServerMessage.ReducerResultMsg(
            requestId = 11u,
            timestamp = Timestamp.UNIX_EPOCH,
            result = ReducerOutcome.InternalError("internal server error"),
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message procedure result returned round trip`() {
        val original = ServerMessage.ProcedureResultMsg(
            status = ProcedureStatus.Returned(byteArrayOf(1, 2, 3)),
            timestamp = Timestamp.fromEpochMicroseconds(1_000_000L),
            totalHostExecutionDuration = TimeDuration(Duration.ZERO),
            requestId = 20u,
        )
        assertServerMessageRoundTrip(original)
    }

    @Test
    fun `server message procedure result internal error round trip`() {
        val original = ServerMessage.ProcedureResultMsg(
            status = ProcedureStatus.InternalError("proc failed"),
            timestamp = Timestamp.UNIX_EPOCH,
            totalHostExecutionDuration = TimeDuration(Duration.ZERO),
            requestId = 21u,
        )
        assertServerMessageRoundTrip(original)
    }

    // ---- Helpers ----

    /** Encode → decode round-trip for ClientMessage. Uses data class equals. */
    private fun roundTripClientMessage(original: ClientMessage): ClientMessage {
        val bytes = ClientMessage.encodeToBytes(original)
        return decodeClientMessage(BsatnReader(bytes))
    }

    /**
     * Encode → decode → re-encode round-trip for ServerMessage.
     * Asserts that the byte representation is identical after a round-trip.
     */
    private fun assertServerMessageRoundTrip(original: ServerMessage) {
        val bytes1 = encodeServerMessage(original)
        val decoded = ServerMessage.decodeFromBytes(bytes1)
        val bytes2 = encodeServerMessage(decoded)
        assertContentEquals(bytes1, bytes2)
    }

    // ---- Test-only decode for ClientMessage (inverse of ClientMessage.encode) ----

    private fun decodeClientMessage(reader: BsatnReader): ClientMessage {
        return when (val tag = reader.readSumTag().toInt()) {
            0 -> ClientMessage.Subscribe(
                requestId = reader.readU32(),
                querySetId = QuerySetId(reader.readU32()),
                queryStrings = List(reader.readArrayLen()) { reader.readString() },
            )
            1 -> ClientMessage.Unsubscribe(
                requestId = reader.readU32(),
                querySetId = QuerySetId(reader.readU32()),
                flags = when (val ft = reader.readSumTag().toInt()) {
                    0 -> UnsubscribeFlags.Default
                    1 -> UnsubscribeFlags.SendDroppedRows
                    else -> error("Unknown UnsubscribeFlags tag: $ft")
                },
            )
            2 -> ClientMessage.OneOffQuery(
                requestId = reader.readU32(),
                queryString = reader.readString(),
            )
            3 -> ClientMessage.CallReducer(
                requestId = reader.readU32(),
                flags = reader.readU8(),
                reducer = reader.readString(),
                args = reader.readByteArray(),
            )
            4 -> ClientMessage.CallProcedure(
                requestId = reader.readU32(),
                flags = reader.readU8(),
                procedure = reader.readString(),
                args = reader.readByteArray(),
            )
            else -> error("Unknown ClientMessage tag: $tag")
        }
    }

    // ---- Test-only encode for ServerMessage (inverse of ServerMessage.decode) ----

    private fun encodeServerMessage(msg: ServerMessage): ByteArray {
        val writer = BsatnWriter()
        when (msg) {
            is ServerMessage.InitialConnection -> {
                writer.writeSumTag(0u)
                msg.identity.encode(writer)
                msg.connectionId.encode(writer)
                writer.writeString(msg.token)
            }
            is ServerMessage.SubscribeApplied -> {
                writer.writeSumTag(1u)
                writer.writeU32(msg.requestId)
                writer.writeU32(msg.querySetId.id)
                encodeQueryRows(writer, msg.rows)
            }
            is ServerMessage.UnsubscribeApplied -> {
                writer.writeSumTag(2u)
                writer.writeU32(msg.requestId)
                writer.writeU32(msg.querySetId.id)
                if (msg.rows != null) {
                    writer.writeSumTag(0u) // Some
                    encodeQueryRows(writer, msg.rows)
                } else {
                    writer.writeSumTag(1u) // None
                }
            }
            is ServerMessage.SubscriptionError -> {
                writer.writeSumTag(3u)
                if (msg.requestId != null) {
                    writer.writeSumTag(0u) // Some
                    writer.writeU32(msg.requestId)
                } else {
                    writer.writeSumTag(1u) // None
                }
                writer.writeU32(msg.querySetId.id)
                writer.writeString(msg.error)
            }
            is ServerMessage.TransactionUpdateMsg -> {
                writer.writeSumTag(4u)
                encodeTransactionUpdate(writer, msg.update)
            }
            is ServerMessage.OneOffQueryResult -> {
                writer.writeSumTag(5u)
                writer.writeU32(msg.requestId)
                when (val r = msg.result) {
                    is QueryResult.Ok -> {
                        writer.writeSumTag(0u)
                        encodeQueryRows(writer, r.rows)
                    }
                    is QueryResult.Err -> {
                        writer.writeSumTag(1u)
                        writer.writeString(r.error)
                    }
                }
            }
            is ServerMessage.ReducerResultMsg -> {
                writer.writeSumTag(6u)
                writer.writeU32(msg.requestId)
                msg.timestamp.encode(writer)
                encodeReducerOutcome(writer, msg.result)
            }
            is ServerMessage.ProcedureResultMsg -> {
                writer.writeSumTag(7u)
                encodeProcedureStatus(writer, msg.status)
                msg.timestamp.encode(writer)
                msg.totalHostExecutionDuration.encode(writer)
                writer.writeU32(msg.requestId)
            }
        }
        return writer.toByteArray()
    }

    private fun encodeQueryRows(writer: BsatnWriter, rows: QueryRows) {
        writer.writeArrayLen(rows.tables.size)
        for (t in rows.tables) {
            writer.writeString(t.table)
            encodeBsatnRowList(writer, t.rows)
        }
    }

    private fun encodeBsatnRowList(writer: BsatnWriter, rowList: BsatnRowList) {
        encodeRowSizeHint(writer, rowList.sizeHint)
        writer.writeU32(rowList.rowsSize.toUInt())
        val reader = rowList.rowsReader
        if (rowList.rowsSize > 0) {
            writer.writeRawBytes(reader.data.copyOfRange(reader.offset, reader.offset + rowList.rowsSize))
        }
    }

    private fun encodeRowSizeHint(writer: BsatnWriter, hint: RowSizeHint) {
        when (hint) {
            is RowSizeHint.FixedSize -> {
                writer.writeSumTag(0u)
                writer.writeU16(hint.size)
            }
            is RowSizeHint.RowOffsets -> {
                writer.writeSumTag(1u)
                writer.writeArrayLen(hint.offsets.size)
                for (o in hint.offsets) writer.writeU64(o)
            }
        }
    }

    private fun encodeTransactionUpdate(writer: BsatnWriter, update: TransactionUpdate) {
        writer.writeArrayLen(update.querySets.size)
        for (qs in update.querySets) {
            writer.writeU32(qs.querySetId.id)
            writer.writeArrayLen(qs.tables.size)
            for (tu in qs.tables) {
                writer.writeString(tu.tableName)
                writer.writeArrayLen(tu.rows.size)
                for (tur in tu.rows) {
                    when (tur) {
                        is TableUpdateRows.PersistentTable -> {
                            writer.writeSumTag(0u)
                            encodeBsatnRowList(writer, tur.inserts)
                            encodeBsatnRowList(writer, tur.deletes)
                        }
                        is TableUpdateRows.EventTable -> {
                            writer.writeSumTag(1u)
                            encodeBsatnRowList(writer, tur.events)
                        }
                    }
                }
            }
        }
    }

    private fun encodeReducerOutcome(writer: BsatnWriter, outcome: ReducerOutcome) {
        when (outcome) {
            is ReducerOutcome.Ok -> {
                writer.writeSumTag(0u)
                writer.writeByteArray(outcome.retValue)
                encodeTransactionUpdate(writer, outcome.transactionUpdate)
            }
            is ReducerOutcome.OkEmpty -> writer.writeSumTag(1u)
            is ReducerOutcome.Err -> {
                writer.writeSumTag(2u)
                writer.writeByteArray(outcome.error)
            }
            is ReducerOutcome.InternalError -> {
                writer.writeSumTag(3u)
                writer.writeString(outcome.message)
            }
        }
    }

    private fun encodeProcedureStatus(writer: BsatnWriter, status: ProcedureStatus) {
        when (status) {
            is ProcedureStatus.Returned -> {
                writer.writeSumTag(0u)
                writer.writeByteArray(status.value)
            }
            is ProcedureStatus.InternalError -> {
                writer.writeSumTag(1u)
                writer.writeString(status.message)
            }
        }
    }
}
