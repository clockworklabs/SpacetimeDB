package com.clockworklabs.spacetimedb

expect fun decompressBrotli(data: ByteArray): ByteArray

expect fun decompressGzip(data: ByteArray): ByteArray
