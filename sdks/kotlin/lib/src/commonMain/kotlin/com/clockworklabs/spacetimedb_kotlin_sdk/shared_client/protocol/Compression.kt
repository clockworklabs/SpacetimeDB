package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

/**
 * Compression tags matching the SpacetimeDB wire protocol.
 * First byte of every WebSocket message indicates compression.
 */
object Compression {
    const val NONE: Byte = 0x00
    const val BROTLI: Byte = 0x01
    const val GZIP: Byte = 0x02
}

/**
 * Strips the compression prefix byte and decompresses if needed.
 * Returns the raw BSATN payload.
 */
expect fun decompressMessage(data: ByteArray): ByteArray
