package com.clockworklabs.spacetimedb

import com.clockworklabs.spacetimedb.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb.protocol.ServerMessage
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class OneOffQueryTest {

    @Test
    fun decodeOneOffQueryOk() {
        val writer = BsatnWriter()
        // ServerMessage tag 5 = OneOffQueryResult
        writer.writeTag(5u)
        // requestId
        writer.writeU32(42u)
        // Result tag 0 = Ok(QueryRows)
        writer.writeTag(0u)
        // QueryRows: array of SingleTableRows (empty)
        writer.writeU32(0u)

        val msg = ServerMessage.decode(writer.toByteArray())
        assertTrue(msg is ServerMessage.OneOffQueryResult)
        assertEquals(42u, msg.requestId)
        assertNotNull(msg.rows)
        assertEquals(0, msg.rows!!.tables.size)
        assertNull(msg.error)
    }

    @Test
    fun decodeOneOffQueryErr() {
        val writer = BsatnWriter()
        // ServerMessage tag 5 = OneOffQueryResult
        writer.writeTag(5u)
        // requestId
        writer.writeU32(99u)
        // Result tag 1 = Err(string)
        writer.writeTag(1u)
        writer.writeString("table not found")

        val msg = ServerMessage.decode(writer.toByteArray())
        assertTrue(msg is ServerMessage.OneOffQueryResult)
        assertEquals(99u, msg.requestId)
        assertNull(msg.rows)
        assertEquals("table not found", msg.error)
    }

    @Test
    fun decodeOneOffQueryOkWithRows() {
        val writer = BsatnWriter()
        writer.writeTag(5u)
        writer.writeU32(7u)
        // Result tag 0 = Ok
        writer.writeTag(0u)
        // QueryRows: 1 table
        writer.writeU32(1u)
        // SingleTableRows: table name (RawIdentifier = string)
        writer.writeString("users")
        // BsatnRowList: RowSizeHint (tag 0 = FixedSize)
        writer.writeTag(0u)
        writer.writeU16(4u)
        // rowsData: 2 rows of 4 bytes each = 8 bytes
        val rowsData = byteArrayOf(1, 2, 3, 4, 5, 6, 7, 8)
        writer.writeByteArray(rowsData)

        val msg = ServerMessage.decode(writer.toByteArray())
        assertTrue(msg is ServerMessage.OneOffQueryResult)
        assertEquals(7u, msg.requestId)
        assertNotNull(msg.rows)
        assertEquals(1, msg.rows!!.tables.size)
        assertEquals("users", msg.rows!!.tables[0].table.value)

        val decodedRows = msg.rows!!.tables[0].rows.decodeRows()
        assertEquals(2, decodedRows.size)
        assertTrue(byteArrayOf(1, 2, 3, 4).contentEquals(decodedRows[0]))
        assertTrue(byteArrayOf(5, 6, 7, 8).contentEquals(decodedRows[1]))
        assertNull(msg.error)
    }
}
