package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode

/**
 * Compression tags matching the SpacetimeDB wire protocol.
 * First byte of every WebSocket message indicates compression.
 */
internal object Compression {
    /** No compression applied. */
    const val NONE: Byte = 0x00
    /** Brotli compression. */
    const val BROTLI: Byte = 0x01
    /** Gzip compression. */
    const val GZIP: Byte = 0x02
}

/**
 * Result of decompressing a message: the payload bytes and the offset at which they start.
 * For compressed messages, [data] is a freshly-allocated array and [offset] is 0.
 * For uncompressed messages, [data] is the original array and [offset] skips the tag byte,
 * avoiding an unnecessary allocation.
 */
internal class DecompressedPayload(val data: ByteArray, val offset: Int = 0) {
    init {
        require(offset in 0..data.size) { "offset $offset out of bounds for data of size ${data.size}" }
    }

    /** Number of usable bytes in the payload (total data size minus the offset). */
    val size: Int get() = data.size - offset
}

/**
 * Strips the compression prefix byte and decompresses if needed.
 * Returns the raw BSATN payload.
 */
internal expect fun decompressMessage(data: ByteArray): DecompressedPayload

/**
 * Default compression mode for this platform.
 * Native targets default to NONE (no decompression support); JVM/Android default to GZIP.
 */
internal expect val defaultCompressionMode: CompressionMode

/**
 * Compression modes supported on this platform.
 * The builder validates that the user-selected mode is in this set.
 */
internal expect val availableCompressionModes: Set<CompressionMode>
