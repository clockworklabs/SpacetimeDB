package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

/**
 * Compression tags matching the SpacetimeDB wire protocol.
 * First byte of every WebSocket message indicates compression.
 */
public object Compression {
    public const val NONE: Byte = 0x00
    public const val BROTLI: Byte = 0x01
    public const val GZIP: Byte = 0x02
}

/**
 * Result of decompressing a message: the payload bytes and the offset at which they start.
 * For compressed messages, [data] is a freshly-allocated array and [offset] is 0.
 * For uncompressed messages, [data] is the original array and [offset] skips the tag byte,
 * avoiding an unnecessary allocation.
 */
public class DecompressedPayload(public val data: ByteArray, public val offset: Int = 0) {
    public val size: Int get() = data.size - offset
}

/**
 * Strips the compression prefix byte and decompresses if needed.
 * Returns the raw BSATN payload.
 */
public expect fun decompressMessage(data: ByteArray): DecompressedPayload

/**
 * Default compression mode for this platform.
 * Native targets default to NONE (no decompression support); JVM/Android default to GZIP.
 */
public expect val defaultCompressionMode: com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode

/**
 * Compression modes supported on this platform.
 * The builder validates that the user-selected mode is in this set.
 */
public expect val availableCompressionModes: Set<com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode>
