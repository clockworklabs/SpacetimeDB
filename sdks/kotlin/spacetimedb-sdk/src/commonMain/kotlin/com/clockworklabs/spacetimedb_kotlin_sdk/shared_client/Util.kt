package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.random.Random
import kotlin.time.Instant

internal fun BigInteger.toHexString(byteWidth: Int): String {
    require(signum() >= 0) { "toHexString requires a non-negative value, got $this" }
    return toString(16).padStart(byteWidth * 2, '0')
}

internal fun parseHexString(hex: String): BigInteger = BigInteger.parseString(hex, 16)
internal fun randomBigInteger(byteLength: Int): BigInteger {
    val bytes = ByteArray(byteLength)
    Random.nextBytes(bytes)
    return BigInteger.fromByteArray(bytes, Sign.POSITIVE)
}


internal fun Instant.Companion.fromEpochMicroseconds(micros: Long): Instant {
    val seconds = micros.floorDiv(1_000_000L)
    val nanos = micros.mod(1_000_000L).toInt() * 1_000
    return fromEpochSeconds(seconds, nanos)
}

private const val MAX_EPOCH_SECONDS_FOR_MICROS = Long.MAX_VALUE / 1_000_000L
private const val MIN_EPOCH_SECONDS_FOR_MICROS = Long.MIN_VALUE / 1_000_000L

internal fun Instant.toEpochMicroseconds(): Long {
    require(epochSeconds in MIN_EPOCH_SECONDS_FOR_MICROS..MAX_EPOCH_SECONDS_FOR_MICROS) {
        "Timestamp $this is outside the representable microsecond range"
    }
    return epochSeconds * 1_000_000L + (nanosecondsOfSecond / 1_000)
}
