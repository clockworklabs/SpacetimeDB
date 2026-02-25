package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import java.math.BigInteger
import kotlin.random.Random
import kotlin.time.Instant

internal fun BigInteger.toHexString(byteWidth: Int): String =
    toString(16).padStart(byteWidth * 2, '0')

internal fun parseHexString(hex: String): BigInteger = BigInteger(hex, 16)
internal fun randomBigInteger(byteLength: Int): BigInteger {
    val bytes = ByteArray(byteLength)
    Random.nextBytes(bytes)
    return BigInteger(1, bytes) // 1 for positive
}

internal fun Instant.Companion.fromEpochMicroseconds(micros: Long): Instant {
    val seconds = micros / 1_000_000
    val microRemainder = (micros % 1_000_000).toInt()
    val nanos = microRemainder * 1_000  // convert back to nanoseconds
    return fromEpochSeconds(seconds, nanos)
}

internal fun Instant.toEpochMicroseconds(): Long {
    return epochSeconds * 1_000_000L + (nanosecondsOfSecond / 1_000)
}
