package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import java.io.ByteArrayOutputStream
import java.util.zip.GZIPOutputStream
import kotlin.test.Test
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class CompressionTest {

    @Test
    fun noneTagReturnsPayloadUnchanged() {
        val payload = byteArrayOf(10, 20, 30, 40)
        val message = byteArrayOf(Compression.NONE) + payload

        val result = decompressMessage(message)
        assertTrue(payload.contentEquals(result))
    }

    @Test
    fun gzipTagDecompressesPayload() {
        val original = "Hello SpacetimeDB".encodeToByteArray()

        // Compress with java.util.zip
        val compressed = ByteArrayOutputStream().use { baos ->
            GZIPOutputStream(baos).use { gzip ->
                gzip.write(original)
            }
            baos.toByteArray()
        }

        val message = byteArrayOf(Compression.GZIP) + compressed
        val result = decompressMessage(message)
        assertTrue(original.contentEquals(result))
    }

    @Test
    fun emptyInputThrows() {
        assertFailsWith<IllegalArgumentException> {
            decompressMessage(byteArrayOf())
        }
    }

    @Test
    fun unknownTagThrows() {
        assertFailsWith<IllegalStateException> {
            decompressMessage(byteArrayOf(0x7F, 1, 2, 3))
        }
    }

    @Test
    fun noneTagEmptyPayload() {
        val message = byteArrayOf(Compression.NONE)
        val result = decompressMessage(message)
        assertTrue(result.isEmpty())
    }
}
