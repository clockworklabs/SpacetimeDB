package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SqlLit
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.SpacetimeUuid
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class SqlLitTest {

    // --- String ---

    @Test
    fun `string literal wraps in quotes`() {
        val lit = SqlLit.string("hello")
        assertTrue(lit.sql.startsWith("'"), "Should start with quote: ${lit.sql}")
        assertTrue(lit.sql.endsWith("'"), "Should end with quote: ${lit.sql}")
        assertTrue(lit.sql.contains("hello"), "Should contain value: ${lit.sql}")
    }

    @Test
    fun `string literal escapes single quotes`() {
        val lit = SqlLit.string("it's")
        // SQL standard: single quotes are escaped by doubling them
        assertTrue(lit.sql.contains("''"), "Should escape single quote: ${lit.sql}")
    }

    @Test
    fun `string literal handles empty string`() {
        val lit = SqlLit.string("")
        assertEquals("''", lit.sql, "Empty string should be two quotes")
    }

    // --- Bool ---

    @Test
    fun `bool true literal`() {
        assertEquals("TRUE", SqlLit.bool(true).sql)
    }

    @Test
    fun `bool false literal`() {
        assertEquals("FALSE", SqlLit.bool(false).sql)
    }

    // --- Numeric types ---

    @Test
    fun `byte literal`() {
        assertEquals("42", SqlLit.byte(42).sql)
        assertEquals("-128", SqlLit.byte(Byte.MIN_VALUE).sql)
        assertEquals("127", SqlLit.byte(Byte.MAX_VALUE).sql)
    }

    @Test
    fun `ubyte literal`() {
        assertEquals("0", SqlLit.ubyte(0u).sql)
        assertEquals("255", SqlLit.ubyte(UByte.MAX_VALUE).sql)
    }

    @Test
    fun `short literal`() {
        assertEquals("1000", SqlLit.short(1000).sql)
        assertEquals("-32768", SqlLit.short(Short.MIN_VALUE).sql)
    }

    @Test
    fun `ushort literal`() {
        assertEquals("0", SqlLit.ushort(0u).sql)
        assertEquals("65535", SqlLit.ushort(UShort.MAX_VALUE).sql)
    }

    @Test
    fun `int literal`() {
        assertEquals("42", SqlLit.int(42).sql)
        assertEquals("0", SqlLit.int(0).sql)
        assertEquals("-1", SqlLit.int(-1).sql)
    }

    @Test
    fun `uint literal`() {
        assertEquals("0", SqlLit.uint(0u).sql)
        assertEquals("4294967295", SqlLit.uint(UInt.MAX_VALUE).sql)
    }

    @Test
    fun `long literal`() {
        assertEquals("0", SqlLit.long(0L).sql)
        assertEquals("9223372036854775807", SqlLit.long(Long.MAX_VALUE).sql)
    }

    @Test
    fun `ulong literal`() {
        assertEquals("0", SqlLit.ulong(0uL).sql)
        assertEquals("18446744073709551615", SqlLit.ulong(ULong.MAX_VALUE).sql)
    }

    @Test
    fun `float literal`() {
        val lit = SqlLit.float(3.14f)
        assertTrue(lit.sql.startsWith("3.14"), "Float should contain value: ${lit.sql}")
    }

    @Test
    fun `double literal`() {
        assertEquals("3.14", SqlLit.double(3.14).sql)
    }

    // --- Identity / ConnectionId / UUID ---

    @Test
    fun `identity literal is hex formatted`() {
        val identity = Identity.zero()
        val lit = SqlLit.identity(identity)
        assertTrue(lit.sql.isNotEmpty(), "Identity literal should not be empty: ${lit.sql}")
        // Zero identity => all zeros hex
        assertTrue(lit.sql.contains("0".repeat(32)), "Zero identity should contain zeros: ${lit.sql}")
    }

    @Test
    fun `identity literal from hex string`() {
        val hex = "ab".repeat(32) // 64 hex chars for 32-byte U256
        val identity = Identity.fromHexString(hex)
        val lit = SqlLit.identity(identity)
        assertTrue(lit.sql.contains("ab"), "Should contain hex value: ${lit.sql}")
    }

    @Test
    fun `connectionId literal is hex formatted`() {
        val connId = ConnectionId.zero()
        val lit = SqlLit.connectionId(connId)
        assertTrue(lit.sql.isNotEmpty(), "ConnectionId literal should not be empty: ${lit.sql}")
    }

    @Test
    fun `connectionId literal from random`() {
        val connId = ConnectionId.random()
        val lit = SqlLit.connectionId(connId)
        assertTrue(lit.sql.isNotEmpty(), "Random connectionId literal should not be empty: ${lit.sql}")
    }

    @Test
    fun `uuid literal is hex formatted`() {
        val uuid = SpacetimeUuid.NIL
        val lit = SqlLit.uuid(uuid)
        assertTrue(lit.sql.contains("0".repeat(32)), "NIL uuid should be all zeros: ${lit.sql}")
    }

    @Test
    fun `uuid literal for random uuid`() {
        val uuid = SpacetimeUuid.random()
        val lit = SqlLit.uuid(uuid)
        assertTrue(lit.sql.isNotEmpty(), "UUID literal should not be empty")
        // Hex literal format is typically 0x... or X'...'
        assertTrue(lit.sql.length > 32, "UUID literal should contain hex representation: ${lit.sql}")
    }
}
