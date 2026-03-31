package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Counter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.SpacetimeUuid
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.UuidVersion
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlin.time.Instant

class SpacetimeUuidTest {

    @Test
    fun `NIL uuid has version Nil`() {
        assertEquals(UuidVersion.Nil, SpacetimeUuid.NIL.getVersion())
    }

    @Test
    fun `MAX uuid has version Max`() {
        assertEquals(UuidVersion.Max, SpacetimeUuid.MAX.getVersion())
    }

    @Test
    fun `random produces V4 uuid`() {
        val uuid = SpacetimeUuid.random()
        assertEquals(UuidVersion.V4, uuid.getVersion())
    }

    @Test
    fun `random produces unique values`() {
        val a = SpacetimeUuid.random()
        val b = SpacetimeUuid.random()
        assertNotEquals(a, b)
    }

    @Test
    fun `parse roundtrips through toString`() {
        val uuid = SpacetimeUuid.random()
        val str = uuid.toString()
        val parsed = SpacetimeUuid.parse(str)
        assertEquals(uuid, parsed)
    }

    @Test
    fun `parse invalid string throws`() {
        assertFailsWith<Exception> {
            SpacetimeUuid.parse("not-a-uuid")
        }
    }

    @Test
    fun `toHexString returns 32 lowercase hex chars`() {
        val uuid = SpacetimeUuid.random()
        val hex = uuid.toHexString()
        assertEquals(32, hex.length, "Hex string should be 32 chars: $hex")
        assertTrue(hex.all { it in '0'..'9' || it in 'a'..'f' }, "Should be lowercase hex: $hex")
    }

    @Test
    fun `toByteArray returns 16 bytes`() {
        val uuid = SpacetimeUuid.random()
        assertEquals(16, uuid.toByteArray().size)
    }

    @Test
    fun `NIL and MAX are distinct`() {
        assertNotEquals(SpacetimeUuid.NIL, SpacetimeUuid.MAX)
    }

    @Test
    fun `compareTo orders NIL before MAX`() {
        assertTrue(SpacetimeUuid.NIL < SpacetimeUuid.MAX)
    }

    @Test
    fun `compareTo is reflexive`() {
        val uuid = SpacetimeUuid.random()
        assertEquals(0, uuid.compareTo(uuid))
    }

    @Test
    fun `fromRandomBytesV4 produces V4 uuid`() {
        val bytes = ByteArray(16) { it.toByte() }
        val uuid = SpacetimeUuid.fromRandomBytesV4(bytes)
        assertEquals(UuidVersion.V4, uuid.getVersion())
    }

    @Test
    fun `fromRandomBytesV4 rejects wrong size`() {
        assertFailsWith<IllegalArgumentException> {
            SpacetimeUuid.fromRandomBytesV4(ByteArray(8))
        }
    }

    @Test
    fun `fromCounterV7 produces V7 uuid`() {
        val counter = Counter(0)
        val now = Timestamp(Instant.fromEpochMilliseconds(System.currentTimeMillis()))
        val randomBytes = ByteArray(4) { 0x42 }
        val uuid = SpacetimeUuid.fromCounterV7(counter, now, randomBytes)
        assertEquals(UuidVersion.V7, uuid.getVersion())
    }

    @Test
    fun `fromCounterV7 increments counter`() {
        val counter = Counter(0)
        val now = Timestamp(Instant.fromEpochMilliseconds(System.currentTimeMillis()))
        val randomBytes = ByteArray(4) { 0x42 }

        val a = SpacetimeUuid.fromCounterV7(counter, now, randomBytes)
        val b = SpacetimeUuid.fromCounterV7(counter, now, randomBytes)
        assertNotEquals(a, b, "Sequential V7 UUIDs should differ due to counter")
        assertTrue(a.getCounter() < b.getCounter(), "Counter should increment")
    }

    @Test
    fun `fromCounterV7 rejects too few random bytes`() {
        val counter = Counter(0)
        val now = Timestamp(Instant.fromEpochMilliseconds(System.currentTimeMillis()))
        assertFailsWith<IllegalArgumentException> {
            SpacetimeUuid.fromCounterV7(counter, now, ByteArray(2))
        }
    }

    @Test
    fun `getCounter returns embedded counter value`() {
        val counter = Counter(42)
        val now = Timestamp(Instant.fromEpochMilliseconds(System.currentTimeMillis()))
        val uuid = SpacetimeUuid.fromCounterV7(counter, now, ByteArray(4) { 0 })
        assertEquals(42, uuid.getCounter(), "getCounter should return the embedded counter")
    }

    @Test
    fun `equals and hashCode are consistent`() {
        val uuid = SpacetimeUuid.random()
        val same = SpacetimeUuid.parse(uuid.toString())
        assertEquals(uuid, same)
        assertEquals(uuid.hashCode(), same.hashCode())
    }

    @Test
    fun `NIL toHexString is all zeros`() {
        assertEquals("00000000000000000000000000000000", SpacetimeUuid.NIL.toHexString())
    }

    @Test
    fun `MAX toHexString is all f`() {
        assertEquals("ffffffffffffffffffffffffffffffff", SpacetimeUuid.MAX.toHexString())
    }
}
