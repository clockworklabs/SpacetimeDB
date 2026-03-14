package com.clockworklabs.spacetimedb

import org.brotli.dec.BrotliInputStream
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.GZIPInputStream

actual fun decompressBrotli(data: ByteArray): ByteArray {
    ByteArrayInputStream(data).use { input ->
        BrotliInputStream(input).use { brotli ->
            ByteArrayOutputStream(data.size * 2).use { output ->
                brotli.copyTo(output)
                return output.toByteArray()
            }
        }
    }
}

actual fun decompressGzip(data: ByteArray): ByteArray {
    ByteArrayInputStream(data).use { input ->
        GZIPInputStream(input).use { gzip ->
            ByteArrayOutputStream(data.size * 2).use { output ->
                gzip.copyTo(output)
                return output.toByteArray()
            }
        }
    }
}
