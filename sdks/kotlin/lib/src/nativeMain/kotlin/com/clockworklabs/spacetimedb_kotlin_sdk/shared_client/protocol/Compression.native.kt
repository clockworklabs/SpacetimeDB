package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode

public actual val defaultCompressionMode: CompressionMode = CompressionMode.NONE

public actual val availableCompressionModes: Set<CompressionMode> =
    setOf(CompressionMode.NONE)

public actual fun decompressMessage(data: ByteArray): ByteArray {
    require(data.isNotEmpty()) { "Empty message" }

    val tag = data[0]
    val payload = data.copyOfRange(1, data.size)

    return when (tag) {
        Compression.NONE -> payload
        // https://github.com/google/brotli/issues/1123
        Compression.BROTLI -> error("Brotli compression not supported on native. Use gzip or none.")
        Compression.GZIP -> error("Gzip decompression not yet implemented for native targets.")
        else -> error("Unknown compression tag: $tag")
    }
}
