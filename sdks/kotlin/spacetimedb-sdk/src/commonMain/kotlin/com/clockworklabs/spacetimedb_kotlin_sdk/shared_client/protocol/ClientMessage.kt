package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.InternalSpacetimeApi
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter

/** Opaque identifier for a subscription query set. */
@InternalSpacetimeApi
public data class QuerySetId(val id: UInt) {
    /** Encodes this value to BSATN. */
    public fun encode(writer: BsatnWriter): Unit = writer.writeU32(id)
}

/** Flags controlling server behavior when unsubscribing. */
internal sealed interface UnsubscribeFlags {
    /** Default unsubscribe behavior (rows are silently dropped). */
    data object Default : UnsubscribeFlags
    /** Request that the server send the dropped rows back before completing. */
    data object SendDroppedRows : UnsubscribeFlags

    /** Encodes this value to BSATN. */
    fun encode(writer: BsatnWriter) {
        when (this) {
            is Default -> writer.writeSumTag(0u)
            is SendDroppedRows -> writer.writeSumTag(1u)
        }
    }
}

/**
 * Messages sent from the client to the SpacetimeDB server.
 * Variant tags match the wire protocol (0=Subscribe, 1=Unsubscribe, 2=OneOffQuery, 3=CallReducer, 4=CallProcedure).
 */
internal sealed interface ClientMessage {

    /** Encodes this message to BSATN. */
    fun encode(writer: BsatnWriter)

    /** Request to subscribe to a set of SQL queries. */
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

    /** Request to unsubscribe from a query set. */
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

    /** A single-shot SQL query that does not create a subscription. */
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

    /** Request to invoke a reducer on the server. */
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

    /** Request to invoke a procedure on the server. */
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
        /** Encodes a [ClientMessage] to a BSATN byte array. */
        fun encodeToBytes(message: ClientMessage): ByteArray {
            val writer = BsatnWriter()
            message.encode(writer)
            return writer.toByteArray()
        }
    }
}
