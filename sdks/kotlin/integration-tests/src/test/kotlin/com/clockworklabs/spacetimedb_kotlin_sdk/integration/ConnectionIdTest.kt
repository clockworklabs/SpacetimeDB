package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ConnectionIdTest {

    // --- Factories ---

    @Test
    fun `zero creates zero connectionId`() {
        val id = ConnectionId.zero()
        assertTrue(id.isZero(), "zero() should be zero")
        assertEquals("0".repeat(32), id.toHexString(), "Zero connId should be 32 zeros")
    }

    @Test
    fun `random creates non-zero connectionId`() {
        val id = ConnectionId.random()
        assertTrue(!id.isZero(), "random() should not be zero")
    }

    @Test
    fun `random creates unique values`() {
        val a = ConnectionId.random()
        val b = ConnectionId.random()
        assertNotEquals(a, b, "Two random connectionIds should differ")
    }

    @Test
    fun `fromHexString parses valid hex`() {
        val hex = "ab".repeat(16) // 32 hex chars = 16 bytes = U128
        val id = ConnectionId.fromHexString(hex)
        assertTrue(id.toHexString().contains("ab"), "Should contain ab")
    }

    @Test
    fun `fromHexString roundtrips`() {
        val hex = "0123456789abcdef".repeat(2) // 32 hex chars
        val id = ConnectionId.fromHexString(hex)
        assertEquals(hex, id.toHexString())
    }

    @Test
    fun `fromHexString rejects invalid hex`() {
        assertFailsWith<Exception> {
            ConnectionId.fromHexString("not-hex!")
        }
    }

    @Test
    fun `fromHexStringOrNull returns null for invalid hex`() {
        val result = ConnectionId.fromHexStringOrNull("not-valid")
        assertNull(result, "Invalid hex should return null")
    }

    @Test
    fun `fromHexStringOrNull returns null for zero hex`() {
        val result = ConnectionId.fromHexStringOrNull("0".repeat(32))
        assertNull(result, "Zero hex should return null (nullIfZero)")
    }

    @Test
    fun `fromHexStringOrNull returns non-null for valid nonzero hex`() {
        val result = ConnectionId.fromHexStringOrNull("ab".repeat(16))
        assertNotNull(result, "Valid nonzero hex should return non-null")
    }

    // --- nullIfZero ---

    @Test
    fun `nullIfZero returns null for zero`() {
        assertNull(ConnectionId.nullIfZero(ConnectionId.zero()))
    }

    @Test
    fun `nullIfZero returns identity for nonzero`() {
        val id = ConnectionId.random()
        assertEquals(id, ConnectionId.nullIfZero(id))
    }

    // --- Conversions ---

    @Test
    fun `toHexString returns 32 lowercase hex chars`() {
        val id = ConnectionId.random()
        val hex = id.toHexString()
        assertEquals(32, hex.length, "Hex should be 32 chars: $hex")
        assertTrue(hex.all { it in '0'..'9' || it in 'a'..'f' }, "Should be lowercase hex: $hex")
    }

    @Test
    fun `toByteArray returns 16 bytes`() {
        val id = ConnectionId.random()
        assertEquals(16, id.toByteArray().size)
    }

    @Test
    fun `zero toByteArray is all zeros`() {
        val bytes = ConnectionId.zero().toByteArray()
        assertTrue(bytes.all { it == 0.toByte() }, "Zero bytes should all be 0")
    }

    @Test
    fun `toString equals toHexString`() {
        val id = ConnectionId.random()
        assertEquals(id.toHexString(), id.toString())
    }

    // --- isZero ---

    @Test
    fun `isZero true for zero`() {
        assertTrue(ConnectionId.zero().isZero())
    }

    @Test
    fun `isZero false for random`() {
        assertTrue(!ConnectionId.random().isZero())
    }

    // --- equals / hashCode ---

    @Test
    fun `equal connectionIds have same hashCode`() {
        val hex = "ab".repeat(16)
        val a = ConnectionId.fromHexString(hex)
        val b = ConnectionId.fromHexString(hex)
        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
    }

    // --- Live connectionId from connection ---

    @Test
    fun `connectionId from connection is non-null`() = kotlinx.coroutines.runBlocking {
        val client = connectToDb()
        assertNotNull(client.conn.connectionId, "connectionId should be non-null after connect")
        client.conn.disconnect()
    }

    @Test
    fun `connectionId from connection has valid hex`() = kotlinx.coroutines.runBlocking {
        val client = connectToDb()
        val hex = client.conn.connectionId!!.toHexString()
        assertEquals(32, hex.length)
        assertTrue(hex.all { it in '0'..'9' || it in 'a'..'f' })
        client.conn.disconnect()
    }
}
