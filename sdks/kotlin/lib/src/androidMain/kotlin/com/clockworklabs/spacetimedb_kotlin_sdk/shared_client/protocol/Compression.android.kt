package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode
import org.brotli.dec.BrotliInputStream
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.GZIPInputStream

public actual val defaultCompressionMode: CompressionMode = CompressionMode.BROTLI

public actual val availableCompressionModes: Set<CompressionMode> =
    setOf(CompressionMode.NONE, CompressionMode.GZIP, CompressionMode.BROTLI)

public actual fun decompressMessage(data: ByteArray): ByteArray {
    require(data.isNotEmpty()) { "Empty message" }

    val tag = data[0]
    val payload = data.copyOfRange(1, data.size)

    return when (tag) {
        Compression.NONE -> payload
        Compression.BROTLI -> {
            val input = BrotliInputStream(ByteArrayInputStream(payload))
            val output = ByteArrayOutputStream()
            input.use { it.copyTo(output) }
            output.toByteArray()
        }
        Compression.GZIP -> {
            val input = GZIPInputStream(ByteArrayInputStream(payload))
            val output = ByteArrayOutputStream()
            input.use { it.copyTo(output) }
            output.toByteArray()
        }
        else -> error("Unknown compression tag: $tag")
    }
}
