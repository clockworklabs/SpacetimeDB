package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.GZIPInputStream
import org.brotli.dec.BrotliInputStream

internal actual fun decompressMessage(data: ByteArray): DecompressedPayload {
    require(data.isNotEmpty()) { "Empty message" }

    return when (val tag = data[0]) {
        Compression.NONE -> DecompressedPayload(data, offset = 1)
        Compression.BROTLI -> {
            val input = BrotliInputStream(ByteArrayInputStream(data, 1, data.size - 1))
            val output = ByteArrayOutputStream()
            input.use { it.copyTo(output) }
            DecompressedPayload(output.toByteArray())
        }
        Compression.GZIP -> {
            val input = GZIPInputStream(ByteArrayInputStream(data, 1, data.size - 1))
            val output = ByteArrayOutputStream()
            input.use { it.copyTo(output) }
            DecompressedPayload(output.toByteArray())
        }
        else -> error("Unknown compression tag: $tag")
    }
}

internal actual val defaultCompressionMode: CompressionMode = CompressionMode.GZIP

internal actual val availableCompressionModes: Set<CompressionMode> =
    setOf(CompressionMode.NONE, CompressionMode.BROTLI, CompressionMode.GZIP)
