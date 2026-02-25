@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter

// --- QuerySetId ---

data class QuerySetId(val id: UInt) {
    fun encode(writer: BsatnWriter) = writer.writeU32(id)
}

// --- UnsubscribeFlags ---
// Sum type: tag 0 = Default (unit), tag 1 = SendDroppedRows (unit)

sealed interface UnsubscribeFlags {
    data object Default : UnsubscribeFlags
    data object SendDroppedRows : UnsubscribeFlags

    fun encode(writer: BsatnWriter) {
        when (this) {
            is Default -> writer.writeSumTag(0u)
            is SendDroppedRows -> writer.writeSumTag(1u)
        }
    }
}

// --- ClientMessage ---
// Sum type matching TS SDK's ClientMessage enum variants in order:
//   tag 0 = Subscribe
//   tag 1 = Unsubscribe
//   tag 2 = OneOffQuery
//   tag 3 = CallReducer
//   tag 4 = CallProcedure

sealed interface ClientMessage {

    fun encode(writer: BsatnWriter)

    data class Subscribe(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val queryStrings: List<String>,
    ) : ClientMessage {
        override fun encode(writer: BsatnWriter) {
            writer.writeSumTag(0u)
            writer.writeU32(requestId)
            querySetId.encode(writer)
            writer.writeArrayLen(queryStrings.size)
            for (s in queryStrings) writer.writeString(s)
        }
    }

    data class Unsubscribe(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val flags: UnsubscribeFlags,
    ) : ClientMessage {
        override fun encode(writer: BsatnWriter) {
            writer.writeSumTag(1u)
            writer.writeU32(requestId)
            querySetId.encode(writer)
            flags.encode(writer)
        }
    }

    data class OneOffQuery(
        val requestId: UInt,
        val queryString: String,
    ) : ClientMessage {
        override fun encode(writer: BsatnWriter) {
            writer.writeSumTag(2u)
            writer.writeU32(requestId)
            writer.writeString(queryString)
        }
    }

    data class CallReducer(
        val requestId: UInt,
        val flags: UByte,
        val reducer: String,
        val args: ByteArray,
    ) : ClientMessage {
        override fun encode(writer: BsatnWriter) {
            writer.writeSumTag(3u)
            writer.writeU32(requestId)
            writer.writeU8(flags)
            writer.writeString(reducer)
            writer.writeByteArray(args)
        }

        override fun equals(other: Any?): Boolean =
            other is CallReducer &&
                requestId == other.requestId &&
                flags == other.flags &&
                reducer == other.reducer &&
                args.contentEquals(other.args)

        override fun hashCode(): Int {
            var result = requestId.hashCode()
            result = 31 * result + flags.hashCode()
            result = 31 * result + reducer.hashCode()
            result = 31 * result + args.contentHashCode()
            return result
        }
    }

    data class CallProcedure(
        val requestId: UInt,
        val flags: UByte,
        val procedure: String,
        val args: ByteArray,
    ) : ClientMessage {
        override fun encode(writer: BsatnWriter) {
            writer.writeSumTag(4u)
            writer.writeU32(requestId)
            writer.writeU8(flags)
            writer.writeString(procedure)
            writer.writeByteArray(args)
        }

        override fun equals(other: Any?): Boolean =
            other is CallProcedure &&
                requestId == other.requestId &&
                flags == other.flags &&
                procedure == other.procedure &&
                args.contentEquals(other.args)

        override fun hashCode(): Int {
            var result = requestId.hashCode()
            result = 31 * result + flags.hashCode()
            result = 31 * result + procedure.hashCode()
            result = 31 * result + args.contentHashCode()
            return result
        }
    }

    companion object {
        fun encodeToBytes(message: ClientMessage): ByteArray {
            val writer = BsatnWriter()
            message.encode(writer)
            return writer.toByteArray()
        }
    }
}
