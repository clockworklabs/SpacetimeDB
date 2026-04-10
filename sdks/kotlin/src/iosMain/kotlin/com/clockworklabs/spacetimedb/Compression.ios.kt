package com.clockworklabs.spacetimedb

import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.alloc
import kotlinx.cinterop.free
import kotlinx.cinterop.nativeHeap
import kotlinx.cinterop.ptr
import kotlinx.cinterop.reinterpret
import kotlinx.cinterop.usePinned
import kotlinx.cinterop.value
import platform.zlib.Z_FINISH
import platform.zlib.Z_OK
import platform.zlib.Z_STREAM_END
import platform.zlib.inflate
import platform.zlib.inflateEnd
import platform.zlib.inflateInit2
import platform.zlib.z_stream

actual fun decompressBrotli(data: ByteArray): ByteArray {
    // Brotli decompression requires Apple's Compression framework interop or a bundled decoder.
    // The SDK defaults to Gzip compression (see buildWsUri), so Brotli is not expected.
    // If a server sends Brotli, this will surface the issue clearly.
    throw UnsupportedOperationException(
        "Brotli decompression is not available on iOS. " +
            "Configure the server connection to use Gzip compression instead."
    )
}

@OptIn(ExperimentalForeignApi::class)
actual fun decompressGzip(data: ByteArray): ByteArray {
    if (data.isEmpty()) return data

    val stream = nativeHeap.alloc<z_stream>()
    try {
        stream.zalloc = null
        stream.zfree = null
        stream.opaque = null
        stream.avail_in = 0u
        stream.next_in = null

        // wbits = MAX_WBITS + 16 (31) tells zlib to expect gzip format
        val initResult = inflateInit2(stream.ptr, 31)
        if (initResult != Z_OK) {
            throw IllegalStateException("zlib inflateInit2 failed: $initResult")
        }

        val chunks = mutableListOf<ByteArray>()
        val outBuf = ByteArray(8192)

        data.usePinned { srcPinned ->
            stream.next_in = srcPinned.addressOf(0).reinterpret()
            stream.avail_in = data.size.toUInt()

            do {
                outBuf.usePinned { dstPinned ->
                    stream.next_out = dstPinned.addressOf(0).reinterpret()
                    stream.avail_out = outBuf.size.toUInt()

                    val ret = inflate(stream.ptr, Z_FINISH)
                    if (ret != Z_OK && ret != Z_STREAM_END) {
                        inflateEnd(stream.ptr)
                        throw IllegalStateException("zlib inflate failed: $ret")
                    }

                    val produced = outBuf.size - stream.avail_out.toInt()
                    if (produced > 0) {
                        chunks.add(outBuf.copyOf(produced))
                    }
                }
            } while (stream.avail_out == 0u)
        }

        inflateEnd(stream.ptr)

        // Concatenate chunks
        val totalSize = chunks.sumOf { it.size }
        val result = ByteArray(totalSize)
        var offset = 0
        for (chunk in chunks) {
            chunk.copyInto(result, offset)
            offset += chunk.size
        }
        return result
    } finally {
        nativeHeap.free(stream)
    }
}
