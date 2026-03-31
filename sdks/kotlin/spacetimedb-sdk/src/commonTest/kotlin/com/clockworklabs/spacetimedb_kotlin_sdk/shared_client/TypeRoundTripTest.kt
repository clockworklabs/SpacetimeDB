package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Counter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ScheduleAt
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.SpacetimeUuid
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.TimeDuration
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.UuidVersion
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class TypeRoundTripTest {
    private fun <T> encodeDecode(encode: (BsatnWriter) -> Unit, decode: (BsatnReader) -> T): T {
        val writer = BsatnWriter()
        encode(writer)
        val reader = BsatnReader(writer.toByteArray())
        val result = decode(reader)
        assertEquals(0, reader.remaining, "All bytes should be consumed")
        return result
    }

    // ---- ConnectionId ----

    @Test
    fun `connection id round trip`() {
        val id = ConnectionId.random()
        val decoded = encodeDecode({ id.encode(it) }, { ConnectionId.decode(it) })
        assertEquals(id, decoded)
    }

    @Test
    fun `connection id zero`() {
        val zero = ConnectionId.zero()
        assertTrue(zero.isZero())
        val decoded = encodeDecode({ zero.encode(it) }, { ConnectionId.decode(it) })
        assertEquals(zero, decoded)
        assertTrue(decoded.isZero())
    }

    @Test
    fun `connection id hex round trip`() {
        val id = ConnectionId.random()
        val hex = id.toHexString()
        val restored = ConnectionId.fromHexString(hex)
        assertEquals(id, restored)
    }

    @Test
    fun `connection id to byte array is little endian`() {
        // ConnectionId with value 1 should have byte[0] = 1, rest zeros
        val id = ConnectionId(BigInteger.ONE)
        val bytes = id.toByteArray()
        assertEquals(16, bytes.size)
        assertEquals(1.toByte(), bytes[0])
        for (i in 1 until 16) {
            assertEquals(0.toByte(), bytes[i], "Byte at index $i should be 0")
        }
    }

    @Test
    fun `connection id null if zero`() {
        assertEquals(ConnectionId.nullIfZero(ConnectionId.zero()), null)
        assertTrue(ConnectionId.nullIfZero(ConnectionId.random()) != null)
    }

    @Test
    fun `connection id max value round trip`() {
        // U128 max = 2^128 - 1 (all bits set)
        val maxU128 = BigInteger.ONE.shl(128) - BigInteger.ONE
        val id = ConnectionId(maxU128)
        val decoded = encodeDecode({ id.encode(it) }, { ConnectionId.decode(it) })
        assertEquals(id, decoded)
        assertEquals("f".repeat(32), decoded.toHexString())
    }

    @Test
    fun `connection id high bit set round trip`() {
        // Value with MSB set — tests BigInteger sign handling
        val highBit = BigInteger.ONE.shl(127)
        val id = ConnectionId(highBit)
        val decoded = encodeDecode({ id.encode(it) }, { ConnectionId.decode(it) })
        assertEquals(id, decoded)
    }

    // ---- Identity ----

    @Test
    fun `identity round trip`() {
        val id = Identity(BigInteger.parseString("12345678901234567890"))
        val decoded = encodeDecode({ id.encode(it) }, { Identity.decode(it) })
        assertEquals(id, decoded)
    }

    @Test
    fun `identity zero`() {
        val zero = Identity.zero()
        val decoded = encodeDecode({ zero.encode(it) }, { Identity.decode(it) })
        assertEquals(zero, decoded)
    }

    @Test
    fun `identity hex round trip`() {
        val id = Identity(BigInteger.parseString("999888777666555444333222111"))
        val hex = id.toHexString()
        assertEquals(64, hex.length, "Identity hex should be 64 chars (32 bytes)")
        val restored = Identity.fromHexString(hex)
        assertEquals(id, restored)
    }

    @Test
    fun `identity to byte array is little endian`() {
        val id = Identity(BigInteger.ONE)
        val bytes = id.toByteArray()
        assertEquals(32, bytes.size)
        assertEquals(1.toByte(), bytes[0])
        for (i in 1 until 32) {
            assertEquals(0.toByte(), bytes[i], "Byte at index $i should be 0")
        }
    }

    @Test
    fun `identity max value round trip`() {
        // U256 max = 2^256 - 1 (all bits set)
        val maxU256 = BigInteger.ONE.shl(256) - BigInteger.ONE
        val id = Identity(maxU256)
        val decoded = encodeDecode({ id.encode(it) }, { Identity.decode(it) })
        assertEquals(id, decoded)
        assertEquals("f".repeat(64), decoded.toHexString())
    }

    @Test
    fun `identity high bit set round trip`() {
        // Value with MSB set — tests BigInteger sign handling
        val highBit = BigInteger.ONE.shl(255)
        val id = Identity(highBit)
        val decoded = encodeDecode({ id.encode(it) }, { Identity.decode(it) })
        assertEquals(id, decoded)
    }

    @Test
    fun `identity compare to ordering`() {
        val small = Identity(BigInteger.ONE)
        val large = Identity(BigInteger.parseString("999999999999999999999999999"))
        assertTrue(small < large)
        assertTrue(large > small)
        assertEquals(0, small.compareTo(small))
    }

    // ---- Timestamp ----

    @Test
    fun `timestamp round trip`() {
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val decoded = encodeDecode({ ts.encode(it) }, { Timestamp.decode(it) })
        assertEquals(ts, decoded)
    }

    @Test
    fun `timestamp epoch`() {
        val epoch = Timestamp.UNIX_EPOCH
        assertEquals(0L, epoch.microsSinceUnixEpoch)
        val decoded = encodeDecode({ epoch.encode(it) }, { Timestamp.decode(it) })
        assertEquals(epoch, decoded)
    }

    @Test
    fun `timestamp negative round trip`() {
        // 1969-12-31T23:59:59.000000Z — 1 second before epoch
        val ts = Timestamp.fromEpochMicroseconds(-1_000_000L)
        val decoded = encodeDecode({ ts.encode(it) }, { Timestamp.decode(it) })
        assertEquals(ts, decoded)
        assertEquals(-1_000_000L, decoded.microsSinceUnixEpoch)
    }

    @Test
    fun `timestamp negative with micros round trip`() {
        // Fractional negative: -0.5 seconds = -500_000 micros
        val ts = Timestamp.fromEpochMicroseconds(-500_000L)
        val decoded = encodeDecode({ ts.encode(it) }, { Timestamp.decode(it) })
        assertEquals(ts, decoded)
        assertEquals(-500_000L, decoded.microsSinceUnixEpoch)
    }

    @Test
    fun `timestamp plus minus duration`() {
        val ts = Timestamp.fromEpochMicroseconds(1_000_000L) // 1 second
        val dur = TimeDuration(500_000.microseconds) // 0.5 seconds
        val later = ts + dur
        assertEquals(1_500_000L, later.microsSinceUnixEpoch)
        val earlier = later - dur
        assertEquals(ts, earlier)
    }

    @Test
    fun `timestamp difference`() {
        val ts1 = Timestamp.fromEpochMicroseconds(3_000_000L)
        val ts2 = Timestamp.fromEpochMicroseconds(1_000_000L)
        val diff = ts1 - ts2
        assertEquals(2_000_000L, diff.micros)
    }

    @Test
    fun `timestamp comparison`() {
        val earlier = Timestamp.fromEpochMicroseconds(100L)
        val later = Timestamp.fromEpochMicroseconds(200L)
        assertTrue(earlier < later)
        assertTrue(later > earlier)
    }

    @Test
    fun `timestamp to iso string epoch`() {
        assertEquals("1970-01-01T00:00:00.000000Z", Timestamp.UNIX_EPOCH.toISOString())
    }

    @Test
    fun `timestamp to iso string pre epoch`() {
        // 1 second before epoch
        val ts = Timestamp.fromEpochMicroseconds(-1_000_000L)
        assertEquals("1969-12-31T23:59:59.000000Z", ts.toISOString())
    }

    @Test
    fun `timestamp to iso string pre epoch fractional`() {
        // 0.5 seconds before epoch
        val ts = Timestamp.fromEpochMicroseconds(-500_000L)
        assertEquals("1969-12-31T23:59:59.500000Z", ts.toISOString())
    }

    @Test
    fun `timestamp to iso string known date`() {
        // 2023-11-14T22:13:20.000000Z = 1_700_000_000_000_000 micros
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        assertEquals("2023-11-14T22:13:20.000000Z", ts.toISOString())
    }

    @Test
    fun `timestamp to iso string microsecond precision`() {
        // 1 second + 123456 microseconds
        val ts = Timestamp.fromEpochMicroseconds(1_123_456L)
        assertEquals("1970-01-01T00:00:01.123456Z", ts.toISOString())
    }

    @Test
    fun `timestamp to iso string pads leading zeros`() {
        // 1 second + 7 microseconds — should pad to 6 digits
        val ts = Timestamp.fromEpochMicroseconds(1_000_007L)
        assertEquals("1970-01-01T00:00:01.000007Z", ts.toISOString())
    }

    @Test
    fun `timestamp to string matches to iso string`() {
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_123_456L)
        assertEquals(ts.toISOString(), ts.toString())
    }

    // ---- TimeDuration ----

    @Test
    fun `time duration round trip`() {
        val dur = TimeDuration(123_456.microseconds)
        val decoded = encodeDecode({ dur.encode(it) }, { TimeDuration.decode(it) })
        assertEquals(dur, decoded)
    }

    @Test
    fun `time duration arithmetic`() {
        val a = TimeDuration(1.seconds)
        val b = TimeDuration(500.milliseconds)
        val sum = a + b
        assertEquals(1_500_000L, sum.micros)
        val diff = a - b
        assertEquals(500_000L, diff.micros)
    }

    @Test
    fun `time duration comparison`() {
        val shorter = TimeDuration(100.milliseconds)
        val longer = TimeDuration(200.milliseconds)
        assertTrue(shorter < longer)
    }

    @Test
    fun `time duration from millis`() {
        val dur = TimeDuration.fromMillis(500)
        assertEquals(500L, dur.millis)
        assertEquals(500_000L, dur.micros)
    }

    @Test
    fun `time duration to string`() {
        val positive = TimeDuration(5_123_456.microseconds)
        assertEquals("+5.123456", positive.toString())

        val negative = TimeDuration((-2_000_000).microseconds)
        assertEquals("-2.000000", negative.toString())
    }

    // ---- ScheduleAt ----

    @Test
    fun `schedule at interval round trip`() {
        val interval = ScheduleAt.interval(5.seconds)
        val decoded = encodeDecode({ interval.encode(it) }, { ScheduleAt.decode(it) })
        assertTrue(decoded is ScheduleAt.Interval)
        assertEquals((interval as ScheduleAt.Interval).duration, decoded.duration)
    }

    @Test
    fun `schedule at time round trip`() {
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val time = ScheduleAt.Time(ts)
        val decoded = encodeDecode({ time.encode(it) }, { ScheduleAt.decode(it) })
        assertTrue(decoded is ScheduleAt.Time)
        assertEquals(ts, decoded.timestamp)
    }

    // ---- SpacetimeUuid ----

    @Test
    fun `spacetime uuid round trip`() {
        val uuid = SpacetimeUuid.random()
        val decoded = encodeDecode({ uuid.encode(it) }, { SpacetimeUuid.decode(it) })
        assertEquals(uuid, decoded)
    }

    @Test
    fun `spacetime uuid nil`() {
        assertEquals(UuidVersion.Nil, SpacetimeUuid.NIL.getVersion())
    }

    @Test
    fun `spacetime uuid v4 detection`() {
        // Build a V4 UUID from known bytes
        val bytes = ByteArray(16) { 0x42 }
        val v4 = SpacetimeUuid.fromRandomBytesV4(bytes)
        assertEquals(UuidVersion.V4, v4.getVersion())
    }

    @Test
    fun `spacetime uuid v7 detection`() {
        val counter = Counter()
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)
        val v7 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(UuidVersion.V7, v7.getVersion())
    }

    @Test
    fun `spacetime uuid v7 counter extraction`() {
        val counter = Counter()
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)

        val uuid0 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(0, uuid0.getCounter())

        val uuid1 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(1, uuid1.getCounter())
    }

    @Test
    fun `spacetime uuid compare to ordering`() {
        val a = SpacetimeUuid.parse("00000000-0000-0000-0000-000000000001")
        val b = SpacetimeUuid.parse("00000000-0000-0000-0000-000000000002")
        assertTrue(a < b)
        assertEquals(0, a.compareTo(a))
    }

    @Test
    fun `spacetime uuid v7 timestamp encoding`() {
        val counter = Counter()
        // 1_700_000_000_000_000 microseconds = 1_700_000_000_000 ms
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)
        val uuid = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        val b = uuid.toByteArray()

        // Extract 48-bit timestamp from bytes 0-5 (big-endian)
        val tsMs = (b[0].toLong() and 0xFF shl 40) or
            (b[1].toLong() and 0xFF shl 32) or
            (b[2].toLong() and 0xFF shl 24) or
            (b[3].toLong() and 0xFF shl 16) or
            (b[4].toLong() and 0xFF shl 8) or
            (b[5].toLong() and 0xFF)
        assertEquals(1_700_000_000_000L, tsMs)
    }

    @Test
    fun `spacetime uuid v7 version and variant bits`() {
        val counter = Counter()
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)
        val uuid = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        val b = uuid.toByteArray()

        // Byte 6 high nibble must be 0x7 (version 7)
        assertEquals(0x07, (b[6].toInt() shr 4) and 0x0F)
        // Byte 8 high 2 bits must be 0b10 (variant RFC 4122)
        assertEquals(0x02, (b[8].toInt() shr 6) and 0x03)
    }

    @Test
    fun `spacetime uuid v7 counter wraparound`() {
        // Counter wraps at 0x7FFF_FFFF
        val counter = Counter(0x7FFF_FFFE)
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)

        val uuid1 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(0x7FFF_FFFE, uuid1.getCounter())

        val uuid2 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(0x7FFF_FFFF, uuid2.getCounter())

        // Next increment wraps to 0
        val uuid3 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(0, uuid3.getCounter())
    }

    @Test
    fun `spacetime uuid v7 round trip`() {
        val counter = Counter()
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)
        val uuid = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        val decoded = encodeDecode({ uuid.encode(it) }, { SpacetimeUuid.decode(it) })
        assertEquals(uuid, decoded)
    }

    // ---- Int128 ----

    @Test
    fun `int128 round trip`() {
        val v = Int128(BigInteger.parseString("170141183460469231731687303715884105727")) // 2^127 - 1
        val decoded = encodeDecode({ v.encode(it) }, { Int128.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `int128 zero round trip`() {
        val v = Int128(BigInteger.ZERO)
        val decoded = encodeDecode({ v.encode(it) }, { Int128.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `int128 negative round trip`() {
        val v = Int128(-BigInteger.ONE.shl(127)) // -2^127 (I128 min)
        val decoded = encodeDecode({ v.encode(it) }, { Int128.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `int128 compare to ordering`() {
        val neg = Int128(-BigInteger.ONE)
        val zero = Int128(BigInteger.ZERO)
        val pos = Int128(BigInteger.ONE)
        assertTrue(neg < zero)
        assertTrue(zero < pos)
        assertEquals(0, zero.compareTo(zero))
    }

    @Test
    fun `int128 to string`() {
        val v = Int128(BigInteger.parseString("42"))
        assertEquals("42", v.toString())
    }

    // ---- UInt128 ----

    @Test
    fun `uint128 round trip`() {
        val v = UInt128(BigInteger.ONE.shl(128) - BigInteger.ONE) // 2^128 - 1
        val decoded = encodeDecode({ v.encode(it) }, { UInt128.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `uint128 zero round trip`() {
        val v = UInt128(BigInteger.ZERO)
        val decoded = encodeDecode({ v.encode(it) }, { UInt128.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `uint128 high bit set round trip`() {
        val v = UInt128(BigInteger.ONE.shl(127))
        val decoded = encodeDecode({ v.encode(it) }, { UInt128.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `uint128 compare to ordering`() {
        val small = UInt128(BigInteger.ONE)
        val large = UInt128(BigInteger.ONE.shl(100))
        assertTrue(small < large)
        assertEquals(0, small.compareTo(small))
    }

    // ---- Int256 ----

    @Test
    fun `int256 round trip`() {
        val v = Int256(BigInteger.ONE.shl(255) - BigInteger.ONE) // 2^255 - 1 (I256 max)
        val decoded = encodeDecode({ v.encode(it) }, { Int256.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `int256 zero round trip`() {
        val v = Int256(BigInteger.ZERO)
        val decoded = encodeDecode({ v.encode(it) }, { Int256.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `int256 negative round trip`() {
        val v = Int256(-BigInteger.ONE.shl(255)) // -2^255 (I256 min)
        val decoded = encodeDecode({ v.encode(it) }, { Int256.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `int256 compare to ordering`() {
        val neg = Int256(-BigInteger.ONE)
        val pos = Int256(BigInteger.ONE)
        assertTrue(neg < pos)
    }

    // ---- UInt256 ----

    @Test
    fun `uint256 round trip`() {
        val v = UInt256(BigInteger.ONE.shl(256) - BigInteger.ONE) // 2^256 - 1
        val decoded = encodeDecode({ v.encode(it) }, { UInt256.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `uint256 zero round trip`() {
        val v = UInt256(BigInteger.ZERO)
        val decoded = encodeDecode({ v.encode(it) }, { UInt256.decode(it) })
        assertEquals(v, decoded)
    }

    @Test
    fun `uint256 high bit set round trip`() {
        val v = UInt256(BigInteger.ONE.shl(255))
        val decoded = encodeDecode({ v.encode(it) }, { UInt256.decode(it) })
        assertEquals(v, decoded)
    }

    // ---- SpacetimeResult ----

    @Test
    fun `spacetime result ok round trip`() {
        val writer = BsatnWriter()
        // Encode: tag 0 + I32
        writer.writeSumTag(0u)
        writer.writeI32(42)
        val reader = BsatnReader(writer.toByteArray())
        val tag = reader.readSumTag().toInt()
        assertEquals(0, tag)
        val value = reader.readI32()
        assertEquals(42, value)
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `spacetime result err round trip`() {
        val writer = BsatnWriter()
        // Encode: tag 1 + String
        writer.writeSumTag(1u)
        writer.writeString("oops")
        val reader = BsatnReader(writer.toByteArray())
        val tag = reader.readSumTag().toInt()
        assertEquals(1, tag)
        val error = reader.readString()
        assertEquals("oops", error)
        assertEquals(0, reader.remaining)
    }

    @Test
    fun `spacetime result ok type`() {
        val result: SpacetimeResult<Int, String> = SpacetimeResult.Ok(42)
        assertIs<SpacetimeResult.Ok<Int>>(result)
        assertEquals(42, result.value)
    }

    @Test
    fun `spacetime result err type`() {
        val result: SpacetimeResult<Int, String> = SpacetimeResult.Err("oops")
        assertIs<SpacetimeResult.Err<String>>(result)
        assertEquals("oops", result.error)
    }

    @Test
    fun `spacetime result when exhaustive`() {
        val ok: SpacetimeResult<Int, String> = SpacetimeResult.Ok(1)
        val err: SpacetimeResult<Int, String> = SpacetimeResult.Err("e")
        // Verify exhaustive when works (sealed interface)
        val okMsg = when (ok) {
            is SpacetimeResult.Ok -> "ok:${ok.value}"
            is SpacetimeResult.Err -> "err:${ok.error}"
        }
        assertEquals("ok:1", okMsg)
        val errMsg = when (err) {
            is SpacetimeResult.Ok -> "ok:${err.value}"
            is SpacetimeResult.Err -> "err:${err.error}"
        }
        assertEquals("err:e", errMsg)
    }
}
