package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

class IdentityTest {

    // --- Factories ---

    @Test
    fun `zero creates zero identity`() {
        val id = Identity.zero()
        assertEquals("0".repeat(64), id.toHexString(), "Zero identity should be 64 zeros")
    }

    @Test
    fun `fromHexString parses valid hex`() {
        val hex = "ab".repeat(32) // 64 hex chars = 32 bytes = U256
        val id = Identity.fromHexString(hex)
        assertTrue(id.toHexString().contains("ab"), "Should contain ab: ${id.toHexString()}")
    }

    @Test
    fun `fromHexString roundtrips`() {
        val hex = "0123456789abcdef".repeat(4) // 64 hex chars
        val id = Identity.fromHexString(hex)
        assertEquals(hex, id.toHexString())
    }

    @Test
    fun `fromHexString rejects invalid hex`() {
        assertFailsWith<Exception> {
            Identity.fromHexString("not-valid-hex")
        }
    }

    // --- Conversions ---

    @Test
    fun `toHexString returns 64 lowercase hex chars`() {
        val hex = "ab".repeat(32)
        val id = Identity.fromHexString(hex)
        val result = id.toHexString()
        assertEquals(64, result.length, "Hex should be 64 chars: $result")
        assertTrue(result.all { it in '0'..'9' || it in 'a'..'f' }, "Should be lowercase hex: $result")
    }

    @Test
    fun `toByteArray returns 32 bytes`() {
        val id = Identity.zero()
        val bytes = id.toByteArray()
        assertEquals(32, bytes.size, "Identity should be 32 bytes")
    }

    @Test
    fun `zero toByteArray is all zeros`() {
        val bytes = Identity.zero().toByteArray()
        assertTrue(bytes.all { it == 0.toByte() }, "Zero identity bytes should all be 0")
    }

    @Test
    fun `toString returns hex string`() {
        val id = Identity.zero()
        assertEquals(id.toHexString(), id.toString())
    }

    // --- Comparison ---

    @Test
    fun `compareTo zero vs nonzero`() {
        val zero = Identity.zero()
        val nonzero = Identity.fromHexString("00".repeat(31) + "01")
        assertTrue(zero < nonzero, "Zero should be less than nonzero")
    }

    @Test
    fun `compareTo equal identities`() {
        val a = Identity.fromHexString("ab".repeat(32))
        val b = Identity.fromHexString("ab".repeat(32))
        assertEquals(0, a.compareTo(b))
    }

    @Test
    fun `compareTo is reflexive`() {
        val id = Identity.fromHexString("cd".repeat(32))
        assertEquals(0, id.compareTo(id))
    }

    // --- equals / hashCode ---

    @Test
    fun `equal identities have same hashCode`() {
        val a = Identity.fromHexString("ab".repeat(32))
        val b = Identity.fromHexString("ab".repeat(32))
        assertEquals(a, b)
        assertEquals(a.hashCode(), b.hashCode())
    }

    @Test
    fun `different identities are not equal`() {
        val a = Identity.fromHexString("ab".repeat(32))
        val b = Identity.fromHexString("cd".repeat(32))
        assertNotEquals(a, b)
    }

    // --- Live identity from connection ---

    @Test
    fun `identity from connection has valid hex string`() = kotlinx.coroutines.runBlocking {
        val client = connectToDb()
        val hex = client.identity.toHexString()
        assertEquals(64, hex.length, "Live identity hex should be 64 chars")
        assertTrue(hex.all { it in '0'..'9' || it in 'a'..'f' }, "Should be valid hex: $hex")
        client.conn.disconnect()
    }

    @Test
    fun `identity from connection has 32-byte array`() = kotlinx.coroutines.runBlocking {
        val client = connectToDb()
        assertEquals(32, client.identity.toByteArray().size)
        client.conn.disconnect()
    }

    @Test
    fun `identity fromHexString roundtrips with live identity`() = kotlinx.coroutines.runBlocking {
        val client = connectToDb()
        val hex = client.identity.toHexString()
        val parsed = Identity.fromHexString(hex)
        assertEquals(client.identity, parsed)
        client.conn.disconnect()
    }
}
