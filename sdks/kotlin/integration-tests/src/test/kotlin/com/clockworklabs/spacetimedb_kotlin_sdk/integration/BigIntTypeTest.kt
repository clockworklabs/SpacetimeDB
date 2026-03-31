package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Int128
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Int256
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.UInt128
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.UInt256
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.BigInteger
import module_bindings.BigIntRow
import module_bindings.InsertBigIntsArgs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

/**
 * Integration tests for generated BigIntRow and InsertBigIntsArgs types
 * that use Int128, UInt128, Int256, UInt256 value classes.
 */
class BigIntTypeTest {

    private val ONE = BigInteger.ONE

    // --- BigIntRow encode/decode round-trip ---

    @Test
    fun `BigIntRow encode decode round-trip with zero values`() {
        val row = BigIntRow(
            id = 1UL,
            valI128 = Int128.ZERO,
            valU128 = UInt128.ZERO,
            valI256 = Int256.ZERO,
            valU256 = UInt256.ZERO,
        )
        val decoded = encodeDecode(row)
        assertEquals(row, decoded)
    }

    @Test
    fun `BigIntRow encode decode round-trip with max values`() {
        val row = BigIntRow(
            id = 42UL,
            valI128 = Int128(ONE.shl(127) - ONE),   // I128 max
            valU128 = UInt128(ONE.shl(128) - ONE),   // U128 max
            valI256 = Int256(ONE.shl(255) - ONE),    // I256 max
            valU256 = UInt256(ONE.shl(256) - ONE),   // U256 max
        )
        val decoded = encodeDecode(row)
        assertEquals(row, decoded)
    }

    @Test
    fun `BigIntRow encode decode round-trip with min signed values`() {
        val row = BigIntRow(
            id = 7UL,
            valI128 = Int128(-ONE.shl(127)),         // I128 min
            valU128 = UInt128.ZERO,
            valI256 = Int256(-ONE.shl(255)),          // I256 min
            valU256 = UInt256.ZERO,
        )
        val decoded = encodeDecode(row)
        assertEquals(row, decoded)
    }

    @Test
    fun `BigIntRow encode decode round-trip with small values`() {
        val row = BigIntRow(
            id = 3UL,
            valI128 = Int128(BigInteger(-999)),
            valU128 = UInt128(BigInteger(12345)),
            valI256 = Int256(BigInteger(-67890)),
            valU256 = UInt256(BigInteger(11111)),
        )
        val decoded = encodeDecode(row)
        assertEquals(row, decoded)
    }

    // --- InsertBigIntsArgs encode/decode round-trip ---

    @Test
    fun `InsertBigIntsArgs encode decode round-trip`() {
        val args = InsertBigIntsArgs(
            valI128 = Int128(BigInteger(42)),
            valU128 = UInt128(BigInteger(100)),
            valI256 = Int256(BigInteger(-200)),
            valU256 = UInt256(BigInteger(300)),
        )
        val bytes = args.encode()
        val reader = BsatnReader(bytes)
        val decoded = InsertBigIntsArgs.decode(reader)
        assertEquals(0, reader.remaining, "All bytes should be consumed")
        assertEquals(args, decoded)
    }

    // --- BigIntRow data class equality ---

    @Test
    fun `BigIntRow equals same values`() {
        val a = makeBigIntRow(1UL, 42)
        val b = makeBigIntRow(1UL, 42)
        assertEquals(a, b)
    }

    @Test
    fun `BigIntRow not equals different i128`() {
        val a = makeBigIntRow(1UL, 42)
        val b = makeBigIntRow(1UL, 99)
        assertNotEquals(a, b)
    }

    @Test
    fun `BigIntRow hashCode consistent with equals`() {
        val a = makeBigIntRow(1UL, 42)
        val b = makeBigIntRow(1UL, 42)
        assertEquals(a.hashCode(), b.hashCode())
    }

    @Test
    fun `BigIntRow toString contains field values`() {
        val row = makeBigIntRow(5UL, 123)
        val str = row.toString()
        assertTrue(str.contains("BigIntRow"), "toString should contain class name: $str")
        assertTrue(str.contains("5"), "toString should contain id: $str")
    }

    @Test
    fun `BigIntRow copy preserves unchanged fields`() {
        val original = makeBigIntRow(1UL, 42)
        val copy = original.copy(id = 99UL)
        assertEquals(99UL, copy.id)
        assertEquals(original.valI128, copy.valI128)
        assertEquals(original.valU128, copy.valU128)
        assertEquals(original.valI256, copy.valI256)
        assertEquals(original.valU256, copy.valU256)
    }

    @Test
    fun `BigIntRow destructuring`() {
        val row = makeBigIntRow(10UL, 77)
        val (id, valI128, valU128, valI256, valU256) = row
        assertEquals(10UL, id)
        assertEquals(Int128(BigInteger(77)), valI128)
        assertEquals(UInt128(BigInteger(77)), valU128)
        assertEquals(Int256(BigInteger(77)), valI256)
        assertEquals(UInt256(BigInteger(77)), valU256)
    }

    // --- Value class type safety ---

    @Test
    fun `value classes are distinct types`() {
        val i128 = Int128(BigInteger(42))
        val u128 = UInt128(BigInteger(42))
        assertNotEquals<Any>(i128, u128)
    }

    // --- Helpers ---

    private fun encodeDecode(row: BigIntRow): BigIntRow {
        val writer = BsatnWriter()
        row.encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val decoded = BigIntRow.decode(reader)
        assertEquals(0, reader.remaining, "All bytes should be consumed")
        return decoded
    }

    private fun makeBigIntRow(id: ULong, v: Int): BigIntRow = BigIntRow(
        id = id,
        valI128 = Int128(BigInteger(v)),
        valU128 = UInt128(BigInteger(v)),
        valI256 = Int256(BigInteger(v)),
        valU256 = UInt256(BigInteger(v)),
    )
}
