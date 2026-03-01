package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnWriter

class ReducerHandle(private val connection: DbConnection) {

    fun call(reducerName: String, args: ByteArray = ByteArray(0), callback: ((ReducerResult) -> Unit)? = null) {
        connection.callReducer(reducerName, args, callback)
    }

    fun call(reducerName: String, writeArgs: (BsatnWriter) -> Unit, callback: ((ReducerResult) -> Unit)? = null) {
        val writer = BsatnWriter()
        writeArgs(writer)
        connection.callReducer(reducerName, writer.toByteArray(), callback)
    }
}
