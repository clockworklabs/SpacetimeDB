package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ProcedureStatus
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QueryResult
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ReducerOutcome
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TransactionUpdate
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ServerMessageTest {

    /** Writes an Identity (U256 = 32 bytes LE) */
    private fun BsatnWriter.writeIdentity(value: BigInteger) = writeU256(value)

    /** Writes a ConnectionId (U128 = 16 bytes LE) */
    private fun BsatnWriter.writeConnectionId(value: BigInteger) = writeU128(value)

    /** Writes a Timestamp (I64 microseconds) */
    private fun BsatnWriter.writeTimestamp(micros: Long) = writeI64(micros)

    /** Writes a TimeDuration (I64 microseconds) */
    private fun BsatnWriter.writeTimeDuration(micros: Long) = writeI64(micros)

    /** Writes an empty QueryRows (array len = 0) */
    private fun BsatnWriter.writeEmptyQueryRows() = writeArrayLen(0)

    /** Writes an empty TransactionUpdate (array len = 0 querySets) */
    private fun BsatnWriter.writeEmptyTransactionUpdate() = writeArrayLen(0)

    // ---- InitialConnection (tag 0) ----

    @Test
    fun `initial connection decode`() {
        val identityValue = BigInteger.parseString("12345678", 16)
        val connIdValue = BigInteger.parseString("ABCD", 16)

        val writer = BsatnWriter()
        writer.writeSumTag(0u) // tag = InitialConnection
        writer.writeIdentity(identityValue)
        writer.writeConnectionId(connIdValue)
        writer.writeString("my-auth-token")

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.InitialConnection>(msg)
        assertEquals(Identity(identityValue), msg.identity)
        assertEquals(ConnectionId(connIdValue), msg.connectionId)
        assertEquals("my-auth-token", msg.token)
    }

    // ---- SubscribeApplied (tag 1) ----

    @Test
    fun `subscribe applied empty rows`() {
        val writer = BsatnWriter()
        writer.writeSumTag(1u) // tag = SubscribeApplied
        writer.writeU32(42u)   // requestId
        writer.writeU32(7u)    // querySetId
        writer.writeEmptyQueryRows()

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.SubscribeApplied>(msg)
        assertEquals(42u, msg.requestId)
        assertEquals(7u, msg.querySetId.id)
        assertTrue(msg.rows.tables.isEmpty())
    }

    // ---- UnsubscribeApplied (tag 2) ----

    @Test
    fun `unsubscribe applied with rows`() {
        val writer = BsatnWriter()
        writer.writeSumTag(2u) // tag = UnsubscribeApplied
        writer.writeU32(10u)   // requestId
        writer.writeU32(3u)    // querySetId
        writer.writeSumTag(0u) // Option::Some
        writer.writeEmptyQueryRows()

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.UnsubscribeApplied>(msg)
        assertEquals(10u, msg.requestId)
        assertEquals(3u, msg.querySetId.id)
        assertNotNull(msg.rows)
    }

    @Test
    fun `unsubscribe applied without rows`() {
        val writer = BsatnWriter()
        writer.writeSumTag(2u) // tag = UnsubscribeApplied
        writer.writeU32(10u)   // requestId
        writer.writeU32(3u)    // querySetId
        writer.writeSumTag(1u) // Option::None

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.UnsubscribeApplied>(msg)
        assertNull(msg.rows)
    }

    // ---- SubscriptionError (tag 3) ----

    @Test
    fun `subscription error with request id`() {
        val writer = BsatnWriter()
        writer.writeSumTag(3u) // tag = SubscriptionError
        writer.writeSumTag(0u) // Option::Some(requestId)
        writer.writeU32(55u)   // requestId
        writer.writeU32(8u)    // querySetId
        writer.writeString("table not found")

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.SubscriptionError>(msg)
        assertEquals(55u, msg.requestId)
        assertEquals(8u, msg.querySetId.id)
        assertEquals("table not found", msg.error)
    }

    @Test
    fun `subscription error without request id`() {
        val writer = BsatnWriter()
        writer.writeSumTag(3u) // tag = SubscriptionError
        writer.writeSumTag(1u) // Option::None
        writer.writeU32(8u)    // querySetId
        writer.writeString("internal error")

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.SubscriptionError>(msg)
        assertNull(msg.requestId)
        assertEquals("internal error", msg.error)
    }

    // ---- TransactionUpdateMsg (tag 4) ----

    @Test
    fun `transaction update empty query sets`() {
        val writer = BsatnWriter()
        writer.writeSumTag(4u) // tag = TransactionUpdateMsg
        writer.writeEmptyTransactionUpdate()

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.TransactionUpdateMsg>(msg)
        assertTrue(msg.update.querySets.isEmpty())
    }

    // ---- OneOffQueryResult (tag 5) ----

    @Test
    fun `one off query result ok`() {
        val writer = BsatnWriter()
        writer.writeSumTag(5u)  // tag = OneOffQueryResult
        writer.writeU32(100u)   // requestId
        writer.writeSumTag(0u)  // Result::Ok
        writer.writeEmptyQueryRows()

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.OneOffQueryResult>(msg)
        assertEquals(100u, msg.requestId)
        assertIs<QueryResult.Ok>(msg.result)
    }

    @Test
    fun `one off query result err`() {
        val writer = BsatnWriter()
        writer.writeSumTag(5u)  // tag = OneOffQueryResult
        writer.writeU32(100u)   // requestId
        writer.writeSumTag(1u)  // Result::Err
        writer.writeString("syntax error in query")

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.OneOffQueryResult>(msg)
        assertEquals(100u, msg.requestId)
        val err = assertIs<QueryResult.Err>(msg.result)
        assertEquals("syntax error in query", err.error)
    }

    // ---- ReducerResultMsg (tag 6) ----

    @Test
    fun `reducer result ok`() {
        val writer = BsatnWriter()
        writer.writeSumTag(6u)        // tag = ReducerResultMsg
        writer.writeU32(20u)          // requestId
        writer.writeTimestamp(1_000_000L) // timestamp
        writer.writeSumTag(0u)        // ReducerOutcome::Ok
        writer.writeByteArray(byteArrayOf()) // retValue (empty)
        writer.writeEmptyTransactionUpdate()

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.ReducerResultMsg>(msg)
        assertEquals(20u, msg.requestId)
        val ok = assertIs<ReducerOutcome.Ok>(msg.result)
        assertTrue(ok.retValue.isEmpty())
        assertTrue(ok.transactionUpdate.querySets.isEmpty())
    }

    @Test
    fun `reducer result ok empty`() {
        val writer = BsatnWriter()
        writer.writeSumTag(6u)        // tag = ReducerResultMsg
        writer.writeU32(21u)          // requestId
        writer.writeTimestamp(2_000_000L)
        writer.writeSumTag(1u)        // ReducerOutcome::OkEmpty

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.ReducerResultMsg>(msg)
        assertIs<ReducerOutcome.OkEmpty>(msg.result)
    }

    @Test
    fun `reducer result err`() {
        val writer = BsatnWriter()
        writer.writeSumTag(6u)        // tag = ReducerResultMsg
        writer.writeU32(22u)          // requestId
        writer.writeTimestamp(3_000_000L)
        writer.writeSumTag(2u)        // ReducerOutcome::Err
        writer.writeByteArray(byteArrayOf(0xDE.toByte(), 0xAD.toByte()))

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.ReducerResultMsg>(msg)
        val err = assertIs<ReducerOutcome.Err>(msg.result)
        assertTrue(err.error.contentEquals(byteArrayOf(0xDE.toByte(), 0xAD.toByte())))
    }

    @Test
    fun `reducer result internal error`() {
        val writer = BsatnWriter()
        writer.writeSumTag(6u)        // tag = ReducerResultMsg
        writer.writeU32(23u)          // requestId
        writer.writeTimestamp(4_000_000L)
        writer.writeSumTag(3u)        // ReducerOutcome::InternalError
        writer.writeString("out of memory")

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.ReducerResultMsg>(msg)
        val err = assertIs<ReducerOutcome.InternalError>(msg.result)
        assertEquals("out of memory", err.message)
    }

    // ---- ProcedureResultMsg (tag 7) ----

    @Test
    fun `procedure result returned`() {
        val writer = BsatnWriter()
        writer.writeSumTag(7u)        // tag = ProcedureResultMsg
        writer.writeSumTag(0u)        // ProcedureStatus::Returned
        writer.writeByteArray(byteArrayOf(42)) // return value
        writer.writeTimestamp(5_000_000L)
        writer.writeTimeDuration(100_000L) // 100ms
        writer.writeU32(50u)          // requestId

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.ProcedureResultMsg>(msg)
        assertEquals(50u, msg.requestId)
        val returned = assertIs<ProcedureStatus.Returned>(msg.status)
        assertTrue(returned.value.contentEquals(byteArrayOf(42)))
    }

    @Test
    fun `procedure result internal error`() {
        val writer = BsatnWriter()
        writer.writeSumTag(7u)        // tag = ProcedureResultMsg
        writer.writeSumTag(1u)        // ProcedureStatus::InternalError
        writer.writeString("procedure failed")
        writer.writeTimestamp(6_000_000L)
        writer.writeTimeDuration(200_000L)
        writer.writeU32(51u)          // requestId

        val msg = ServerMessage.decodeFromBytes(writer.toByteArray())
        assertIs<ServerMessage.ProcedureResultMsg>(msg)
        assertEquals(51u, msg.requestId)
        val err = assertIs<ProcedureStatus.InternalError>(msg.status)
        assertEquals("procedure failed", err.message)
    }

    // ---- Unknown tag ----

    @Test
    fun `unknown tag throws`() {
        val writer = BsatnWriter()
        writer.writeSumTag(255u) // invalid tag

        assertFailsWith<IllegalStateException> {
            ServerMessage.decodeFromBytes(writer.toByteArray())
        }
    }

    // ---- ReducerOutcome equality ----

    @Test
    fun `reducer outcome ok equality`() {
        val a = ReducerOutcome.Ok(byteArrayOf(1, 2), TransactionUpdate(emptyList()))
        val b = ReducerOutcome.Ok(byteArrayOf(1, 2), TransactionUpdate(emptyList()))
        val c = ReducerOutcome.Ok(byteArrayOf(3, 4), TransactionUpdate(emptyList()))

        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
        assertTrue(a != c)
    }

    @Test
    fun `reducer outcome err equality`() {
        val a = ReducerOutcome.Err(byteArrayOf(1, 2))
        val b = ReducerOutcome.Err(byteArrayOf(1, 2))
        val c = ReducerOutcome.Err(byteArrayOf(3, 4))

        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
        assertTrue(a != c)
    }

    @Test
    fun `procedure status returned equality`() {
        val a = ProcedureStatus.Returned(byteArrayOf(10))
        val b = ProcedureStatus.Returned(byteArrayOf(10))
        val c = ProcedureStatus.Returned(byteArrayOf(20))

        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
        assertTrue(a != c)
    }
}
