package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.*
import com.ionspin.kotlin.bignum.integer.BigInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlin.uuid.Uuid

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
    fun connectionIdRoundTrip() {
        val id = ConnectionId.random()
        val decoded = encodeDecode({ id.encode(it) }, { ConnectionId.decode(it) })
        assertEquals(id, decoded)
    }

    @Test
    fun connectionIdZero() {
        val zero = ConnectionId.zero()
        assertTrue(zero.isZero())
        val decoded = encodeDecode({ zero.encode(it) }, { ConnectionId.decode(it) })
        assertEquals(zero, decoded)
        assertTrue(decoded.isZero())
    }

    @Test
    fun connectionIdHexRoundTrip() {
        val id = ConnectionId.random()
        val hex = id.toHexString()
        val restored = ConnectionId.fromHexString(hex)
        assertEquals(id, restored)
    }

    @Test
    fun connectionIdToByteArrayIsLittleEndian() {
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
    fun connectionIdNullIfZero() {
        assertTrue(ConnectionId.nullIfZero(ConnectionId.zero()) == null)
        assertTrue(ConnectionId.nullIfZero(ConnectionId.random()) != null)
    }

    // ---- Identity ----

    @Test
    fun identityRoundTrip() {
        val id = Identity(BigInteger.parseString("12345678901234567890"))
        val decoded = encodeDecode({ id.encode(it) }, { Identity.decode(it) })
        assertEquals(id, decoded)
    }

    @Test
    fun identityZero() {
        val zero = Identity.zero()
        val decoded = encodeDecode({ zero.encode(it) }, { Identity.decode(it) })
        assertEquals(zero, decoded)
    }

    @Test
    fun identityHexRoundTrip() {
        val id = Identity(BigInteger.parseString("999888777666555444333222111"))
        val hex = id.toHexString()
        assertEquals(64, hex.length, "Identity hex should be 64 chars (32 bytes)")
        val restored = Identity.fromHexString(hex)
        assertEquals(id, restored)
    }

    @Test
    fun identityToByteArrayIsLittleEndian() {
        val id = Identity(BigInteger.ONE)
        val bytes = id.toByteArray()
        assertEquals(32, bytes.size)
        assertEquals(1.toByte(), bytes[0])
        for (i in 1 until 32) {
            assertEquals(0.toByte(), bytes[i], "Byte at index $i should be 0")
        }
    }

    @Test
    fun identityCompareToOrdering() {
        val small = Identity(BigInteger.ONE)
        val large = Identity(BigInteger.parseString("999999999999999999999999999"))
        assertTrue(small < large)
        assertTrue(large > small)
        assertEquals(0, small.compareTo(small))
    }

    // ---- Timestamp ----

    @Test
    fun timestampRoundTrip() {
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val decoded = encodeDecode({ ts.encode(it) }, { Timestamp.decode(it) })
        assertEquals(ts, decoded)
    }

    @Test
    fun timestampEpoch() {
        val epoch = Timestamp.UNIX_EPOCH
        assertEquals(0L, epoch.microsSinceUnixEpoch)
        val decoded = encodeDecode({ epoch.encode(it) }, { Timestamp.decode(it) })
        assertEquals(epoch, decoded)
    }

    @Test
    fun timestampPlusMinusDuration() {
        val ts = Timestamp.fromEpochMicroseconds(1_000_000L) // 1 second
        val dur = TimeDuration(500_000.microseconds) // 0.5 seconds
        val later = ts + dur
        assertEquals(1_500_000L, later.microsSinceUnixEpoch)
        val earlier = later - dur
        assertEquals(ts, earlier)
    }

    @Test
    fun timestampDifference() {
        val ts1 = Timestamp.fromEpochMicroseconds(3_000_000L)
        val ts2 = Timestamp.fromEpochMicroseconds(1_000_000L)
        val diff = ts1 - ts2
        assertEquals(2_000_000L, diff.micros)
    }

    @Test
    fun timestampComparison() {
        val earlier = Timestamp.fromEpochMicroseconds(100L)
        val later = Timestamp.fromEpochMicroseconds(200L)
        assertTrue(earlier < later)
        assertTrue(later > earlier)
    }

    @Test
    fun timestampToISOStringEpoch() {
        assertEquals("1970-01-01T00:00:00.000000Z", Timestamp.UNIX_EPOCH.toISOString())
    }

    @Test
    fun timestampToISOStringKnownDate() {
        // 2023-11-14T22:13:20.000000Z = 1_700_000_000_000_000 micros
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        assertEquals("2023-11-14T22:13:20.000000Z", ts.toISOString())
    }

    @Test
    fun timestampToISOStringMicrosecondPrecision() {
        // 1 second + 123456 microseconds
        val ts = Timestamp.fromEpochMicroseconds(1_123_456L)
        assertEquals("1970-01-01T00:00:01.123456Z", ts.toISOString())
    }

    @Test
    fun timestampToISOStringPadsLeadingZeros() {
        // 1 second + 7 microseconds — should pad to 6 digits
        val ts = Timestamp.fromEpochMicroseconds(1_000_007L)
        assertEquals("1970-01-01T00:00:01.000007Z", ts.toISOString())
    }

    @Test
    fun timestampToStringMatchesToISOString() {
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_123_456L)
        assertEquals(ts.toISOString(), ts.toString())
    }

    // ---- TimeDuration ----

    @Test
    fun timeDurationRoundTrip() {
        val dur = TimeDuration(123_456.microseconds)
        val decoded = encodeDecode({ dur.encode(it) }, { TimeDuration.decode(it) })
        assertEquals(dur, decoded)
    }

    @Test
    fun timeDurationArithmetic() {
        val a = TimeDuration(1.seconds)
        val b = TimeDuration(500.milliseconds)
        val sum = a + b
        assertEquals(1_500_000L, sum.micros)
        val diff = a - b
        assertEquals(500_000L, diff.micros)
    }

    @Test
    fun timeDurationComparison() {
        val shorter = TimeDuration(100.milliseconds)
        val longer = TimeDuration(200.milliseconds)
        assertTrue(shorter < longer)
    }

    @Test
    fun timeDurationFromMillis() {
        val dur = TimeDuration.fromMillis(500)
        assertEquals(500L, dur.millis)
        assertEquals(500_000L, dur.micros)
    }

    @Test
    fun timeDurationToString() {
        val positive = TimeDuration(5_123_456.microseconds)
        assertEquals("+5.123456", positive.toString())

        val negative = TimeDuration((-2_000_000).microseconds)
        assertEquals("-2.000000", negative.toString())
    }

    // ---- ScheduleAt ----

    @Test
    fun scheduleAtIntervalRoundTrip() {
        val interval = ScheduleAt.interval(5.seconds)
        val decoded = encodeDecode({ interval.encode(it) }, { ScheduleAt.decode(it) })
        assertTrue(decoded is ScheduleAt.Interval)
        assertEquals((interval as ScheduleAt.Interval).duration, decoded.duration)
    }

    @Test
    fun scheduleAtTimeRoundTrip() {
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val time = ScheduleAt.Time(ts)
        val decoded = encodeDecode({ time.encode(it) }, { ScheduleAt.decode(it) })
        assertTrue(decoded is ScheduleAt.Time)
        assertEquals(ts, decoded.timestamp)
    }

    // ---- SpacetimeUuid ----

    @Test
    fun spacetimeUuidRoundTrip() {
        val uuid = SpacetimeUuid.random()
        val decoded = encodeDecode({ uuid.encode(it) }, { SpacetimeUuid.decode(it) })
        assertEquals(uuid, decoded)
    }

    @Test
    fun spacetimeUuidNil() {
        assertEquals(UuidVersion.Nil, SpacetimeUuid.NIL.getVersion())
    }

    @Test
    fun spacetimeUuidV4Detection() {
        // Build a V4 UUID from known bytes
        val bytes = ByteArray(16) { 0x42 }
        val v4 = SpacetimeUuid.fromRandomBytesV4(bytes)
        assertEquals(UuidVersion.V4, v4.getVersion())
    }

    @Test
    fun spacetimeUuidV7Detection() {
        val counter = Counter()
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)
        val v7 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(UuidVersion.V7, v7.getVersion())
    }

    @Test
    fun spacetimeUuidV7CounterExtraction() {
        val counter = Counter()
        val ts = Timestamp.fromEpochMicroseconds(1_700_000_000_000_000L)
        val randomBytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)

        val uuid0 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(0, uuid0.getCounter())

        val uuid1 = SpacetimeUuid.fromCounterV7(counter, ts, randomBytes)
        assertEquals(1, uuid1.getCounter())
    }

    @Test
    fun spacetimeUuidCompareToOrdering() {
        val a = SpacetimeUuid.parse("00000000-0000-0000-0000-000000000001")
        val b = SpacetimeUuid.parse("00000000-0000-0000-0000-000000000002")
        assertTrue(a < b)
        assertEquals(0, a.compareTo(a))
    }
}
