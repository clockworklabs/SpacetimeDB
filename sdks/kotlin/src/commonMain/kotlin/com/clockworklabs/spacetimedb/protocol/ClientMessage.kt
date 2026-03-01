package com.clockworklabs.spacetimedb.protocol

import com.clockworklabs.spacetimedb.bsatn.BsatnWriter

sealed class ClientMessage {
    data class Subscribe(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val queryStrings: List<String>,
    ) : ClientMessage()

    data class Unsubscribe(
        val requestId: UInt,
        val querySetId: QuerySetId,
        val flags: UByte = 0u,
    ) : ClientMessage()

    data class OneOffQuery(
        val requestId: UInt,
        val queryString: String,
    ) : ClientMessage()

    data class CallReducer(
        val requestId: UInt,
        val reducer: String,
        val args: ByteArray,
        val flags: UByte = 0u,
    ) : ClientMessage() {
        override fun equals(other: Any?): Boolean =
            other is CallReducer && requestId == other.requestId &&
                reducer == other.reducer && args.contentEquals(other.args) &&
                flags == other.flags

        override fun hashCode(): Int {
            var result = requestId.hashCode()
            result = 31 * result + reducer.hashCode()
            result = 31 * result + args.contentHashCode()
            result = 31 * result + flags.hashCode()
            return result
        }
    }

    data class CallProcedure(
        val requestId: UInt,
        val procedure: String,
        val args: ByteArray,
        val flags: UByte = 0u,
    ) : ClientMessage() {
        override fun equals(other: Any?): Boolean =
            other is CallProcedure && requestId == other.requestId &&
                procedure == other.procedure && args.contentEquals(other.args) &&
                flags == other.flags

        override fun hashCode(): Int {
            var result = requestId.hashCode()
            result = 31 * result + procedure.hashCode()
            result = 31 * result + args.contentHashCode()
            result = 31 * result + flags.hashCode()
            return result
        }
    }

    fun encode(): ByteArray {
        val writer = BsatnWriter()
        when (this) {
            is Subscribe -> {
                writer.writeTag(0u)
                writer.writeU32(requestId)
                QuerySetId.write(writer, querySetId)
                writer.writeArray(queryStrings) { w, s -> w.writeString(s) }
            }
            is Unsubscribe -> {
                writer.writeTag(1u)
                writer.writeU32(requestId)
                QuerySetId.write(writer, querySetId)
                writer.writeU8(flags)
            }
            is OneOffQuery -> {
                writer.writeTag(2u)
                writer.writeU32(requestId)
                writer.writeString(queryString)
            }
            is CallReducer -> {
                writer.writeTag(3u)
                writer.writeU32(requestId)
                writer.writeU8(flags)
                writer.writeString(reducer)
                writer.writeByteArray(args)
            }
            is CallProcedure -> {
                writer.writeTag(4u)
                writer.writeU32(requestId)
                writer.writeU8(flags)
                writer.writeString(procedure)
                writer.writeByteArray(args)
            }
        }
        return writer.toByteArray()
    }
}
