package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import java.io.ByteArrayOutputStream
import java.util.zip.GZIPOutputStream
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlin.test.assertTrue

class CompressionTest {

    /** Extract the effective payload bytes from a [DecompressedPayload]. */
    private fun DecompressedPayload.toPayloadBytes(): ByteArray =
        data.copyOfRange(offset, data.size)

    @Test
    fun `none tag returns payload unchanged`() {
        val payload = byteArrayOf(10, 20, 30, 40)
        val message = byteArrayOf(Compression.NONE) + payload

        val result = decompressMessage(message)
        // Zero-copy: result references the original array with offset=1
        assertSame(result.data, message, "NONE should return the original array (zero-copy)")
        assertEquals(1, result.offset)
        assertTrue(payload.contentEquals(result.toPayloadBytes()))
    }

    @Test
    fun `gzip tag decompresses payload`() {
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
        assertEquals(0, result.offset)
        assertTrue(original.contentEquals(result.toPayloadBytes()))
    }

    @Test
    fun `empty input throws`() {
        assertFailsWith<IllegalArgumentException> {
            decompressMessage(byteArrayOf())
        }
    }

    @Test
    fun `brotli tag rejects invalid data`() {
        // Brotli decoder is wired up — invalid data throws IOException (not IllegalStateException)
        assertFailsWith<java.io.IOException> {
            decompressMessage(byteArrayOf(Compression.BROTLI, 1, 2, 3))
        }
    }

    @Test
    fun `unknown tag throws`() {
        assertFailsWith<IllegalStateException> {
            decompressMessage(byteArrayOf(0x7F, 1, 2, 3))
        }
    }

    @Test
    fun `none tag empty payload`() {
        val message = byteArrayOf(Compression.NONE)
        val result = decompressMessage(message)
        assertEquals(0, result.size)
    }
}
