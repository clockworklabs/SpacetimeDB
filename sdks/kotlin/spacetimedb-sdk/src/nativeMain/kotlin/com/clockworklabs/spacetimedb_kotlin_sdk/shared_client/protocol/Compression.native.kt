package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode

internal actual val defaultCompressionMode: CompressionMode = CompressionMode.NONE

internal actual val availableCompressionModes: Set<CompressionMode> =
    setOf(CompressionMode.NONE)

internal actual fun decompressMessage(data: ByteArray): DecompressedPayload {
    require(data.isNotEmpty()) { "Empty message" }

    return when (val tag = data[0]) {
        Compression.NONE -> DecompressedPayload(data, offset = 1)
        // https://github.com/google/brotli/issues/1123
        Compression.BROTLI -> error("Brotli compression not supported on native.")
        Compression.GZIP -> error("Gzip compression not supported on native.")
        else -> error("Unknown compression tag: $tag")
    }
}
