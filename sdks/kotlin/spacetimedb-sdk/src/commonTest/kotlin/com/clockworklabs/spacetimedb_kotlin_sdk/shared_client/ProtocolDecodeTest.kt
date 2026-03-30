package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnReader
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.BsatnRowList
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.DecompressedPayload
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ProcedureStatus
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QueryRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ReducerOutcome
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.RowSizeHint
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.SingleTableRows
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.TableUpdateRows
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertTrue

class ProtocolDecodeTest {

    // ---- RowSizeHint ----

    @Test
    fun `row size hint fixed size decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(0u) // tag = FixedSize
        writer.writeU16(4u)    // 4 bytes per row

        val hint = RowSizeHint.decode(BsatnReader(writer.toByteArray()))
        val fixed = assertIs<RowSizeHint.FixedSize>(hint)
        assertEquals(4u.toUShort(), fixed.size)
    }

    @Test
    fun `row size hint row offsets decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(1u) // tag = RowOffsets
        writer.writeArrayLen(3)
        writer.writeU64(0uL)
        writer.writeU64(10uL)
        writer.writeU64(25uL)

        val hint = RowSizeHint.decode(BsatnReader(writer.toByteArray()))
        val offsets = assertIs<RowSizeHint.RowOffsets>(hint)
        assertEquals(listOf(0uL, 10uL, 25uL), offsets.offsets)
    }

    @Test
    fun `row size hint unknown tag throws`() {
        val writer = BsatnWriter()
        writer.writeSumTag(99u) // invalid tag

        assertFailsWith<IllegalStateException> {
            RowSizeHint.decode(BsatnReader(writer.toByteArray()))
        }
    }

    // ---- BsatnRowList ----

    @Test
    fun `bsatn row list decode with fixed size`() {
        val writer = BsatnWriter()
        // RowSizeHint::FixedSize(4)
        writer.writeSumTag(0u)
        writer.writeU16(4u)
        // Rows data: U32 length prefix + raw bytes
        val rowData = byteArrayOf(1, 2, 3, 4, 5, 6, 7, 8) // 2 rows of 4 bytes
        writer.writeU32(rowData.size.toUInt())
        writer.writeRawBytes(rowData)

        val rowList = BsatnRowList.decode(BsatnReader(writer.toByteArray()))
        assertIs<RowSizeHint.FixedSize>(rowList.sizeHint)
        assertEquals(8, rowList.rowsSize)
    }

    @Test
    fun `bsatn row list decode with row offsets`() {
        val writer = BsatnWriter()
        // RowSizeHint::RowOffsets([0, 5])
        writer.writeSumTag(1u)
        writer.writeArrayLen(2)
        writer.writeU64(0uL)
        writer.writeU64(5uL)
        // Rows data
        val rowData = byteArrayOf(10, 20, 30, 40, 50, 60, 70, 80, 90)
        writer.writeU32(rowData.size.toUInt())
        writer.writeRawBytes(rowData)

        val rowList = BsatnRowList.decode(BsatnReader(writer.toByteArray()))
        assertIs<RowSizeHint.RowOffsets>(rowList.sizeHint)
        assertEquals(9, rowList.rowsSize)
    }

    @Test
    fun `bsatn row list decode overflow length throws`() {
        val writer = BsatnWriter()
        // RowSizeHint::FixedSize(4)
        writer.writeSumTag(0u)
        writer.writeU16(4u)
        // Length that overflows Int: 0x80000000 (2,147,483,648)
        writer.writeU32(0x8000_0000u)
        // No actual row data — the check should fire before reading

        assertFailsWith<IllegalStateException> {
            BsatnRowList.decode(BsatnReader(writer.toByteArray()))
        }
    }

    // ---- SingleTableRows ----

    @Test
    fun `single table rows decode`() {
        val writer = BsatnWriter()
        writer.writeString("Players")
        // BsatnRowList: FixedSize(4), 4 bytes of data
        writer.writeSumTag(0u)
        writer.writeU16(4u)
        writer.writeU32(4u)
        writer.writeRawBytes(byteArrayOf(0, 0, 0, 42))

        val rows = SingleTableRows.decode(BsatnReader(writer.toByteArray()))
        assertEquals("Players", rows.table)
        assertEquals(4, rows.rows.rowsSize)
    }

    // ---- QueryRows ----

    @Test
    fun `query rows decode empty`() {
        val writer = BsatnWriter()
        writer.writeArrayLen(0)

        val qr = QueryRows.decode(BsatnReader(writer.toByteArray()))
        assertTrue(qr.tables.isEmpty())
    }

    @Test
    fun `query rows decode with tables`() {
        val writer = BsatnWriter()
        writer.writeArrayLen(2)
        // Table 1
        writer.writeString("Players")
        writer.writeSumTag(0u); writer.writeU16(4u) // FixedSize(4)
        writer.writeU32(0u) // 0 bytes of row data
        // Table 2
        writer.writeString("Items")
        writer.writeSumTag(0u); writer.writeU16(8u) // FixedSize(8)
        writer.writeU32(0u) // 0 bytes of row data

        val qr = QueryRows.decode(BsatnReader(writer.toByteArray()))
        assertEquals(2, qr.tables.size)
        assertEquals("Players", qr.tables[0].table)
        assertEquals("Items", qr.tables[1].table)
    }

    // ---- TableUpdateRows ----

    @Test
    fun `table update rows persistent table decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(0u) // tag = PersistentTable
        // inserts: BsatnRowList
        writer.writeSumTag(0u); writer.writeU16(4u) // FixedSize(4)
        writer.writeU32(4u)
        writer.writeRawBytes(byteArrayOf(1, 0, 0, 0)) // one I32 row
        // deletes: BsatnRowList
        writer.writeSumTag(0u); writer.writeU16(4u) // FixedSize(4)
        writer.writeU32(0u) // no deletes

        val update = TableUpdateRows.decode(BsatnReader(writer.toByteArray()))
        val pt = assertIs<TableUpdateRows.PersistentTable>(update)
        assertEquals(4, pt.inserts.rowsSize)
        assertEquals(0, pt.deletes.rowsSize)
    }

    @Test
    fun `table update rows event table decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(1u) // tag = EventTable
        // events: BsatnRowList
        writer.writeSumTag(0u); writer.writeU16(4u) // FixedSize(4)
        writer.writeU32(8u)
        writer.writeRawBytes(byteArrayOf(1, 0, 0, 0, 2, 0, 0, 0))

        val update = TableUpdateRows.decode(BsatnReader(writer.toByteArray()))
        val et = assertIs<TableUpdateRows.EventTable>(update)
        assertEquals(8, et.events.rowsSize)
    }

    @Test
    fun `table update rows unknown tag throws`() {
        val writer = BsatnWriter()
        writer.writeSumTag(99u)

        assertFailsWith<IllegalStateException> {
            TableUpdateRows.decode(BsatnReader(writer.toByteArray()))
        }
    }

    // ---- ReducerOutcome ----

    @Test
    fun `reducer outcome ok decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(0u) // tag = Ok
        writer.writeByteArray(byteArrayOf(42)) // retValue
        writer.writeArrayLen(0) // empty TransactionUpdate

        val outcome = ReducerOutcome.decode(BsatnReader(writer.toByteArray()))
        val ok = assertIs<ReducerOutcome.Ok>(outcome)
        assertTrue(ok.retValue.contentEquals(byteArrayOf(42)))
        assertTrue(ok.transactionUpdate.querySets.isEmpty())
    }

    @Test
    fun `reducer outcome ok empty decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(1u) // tag = OkEmpty

        val outcome = ReducerOutcome.decode(BsatnReader(writer.toByteArray()))
        assertIs<ReducerOutcome.OkEmpty>(outcome)
    }

    @Test
    fun `reducer outcome err decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(2u) // tag = Err
        writer.writeByteArray(byteArrayOf(0xDE.toByte()))

        val outcome = ReducerOutcome.decode(BsatnReader(writer.toByteArray()))
        val err = assertIs<ReducerOutcome.Err>(outcome)
        assertTrue(err.error.contentEquals(byteArrayOf(0xDE.toByte())))
    }

    @Test
    fun `reducer outcome internal error decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(3u) // tag = InternalError
        writer.writeString("panic in reducer")

        val outcome = ReducerOutcome.decode(BsatnReader(writer.toByteArray()))
        val err = assertIs<ReducerOutcome.InternalError>(outcome)
        assertEquals("panic in reducer", err.message)
    }

    @Test
    fun `reducer outcome unknown tag throws`() {
        val writer = BsatnWriter()
        writer.writeSumTag(99u)

        assertFailsWith<IllegalStateException> {
            ReducerOutcome.decode(BsatnReader(writer.toByteArray()))
        }
    }

    // ---- ProcedureStatus ----

    @Test
    fun `procedure status returned decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(0u) // tag = Returned
        writer.writeByteArray(byteArrayOf(1, 2, 3))

        val status = ProcedureStatus.decode(BsatnReader(writer.toByteArray()))
        val returned = assertIs<ProcedureStatus.Returned>(status)
        assertTrue(returned.value.contentEquals(byteArrayOf(1, 2, 3)))
    }

    @Test
    fun `procedure status internal error decode`() {
        val writer = BsatnWriter()
        writer.writeSumTag(1u) // tag = InternalError
        writer.writeString("procedure crashed")

        val status = ProcedureStatus.decode(BsatnReader(writer.toByteArray()))
        val err = assertIs<ProcedureStatus.InternalError>(status)
        assertEquals("procedure crashed", err.message)
    }

    @Test
    fun `procedure status unknown tag throws`() {
        val writer = BsatnWriter()
        writer.writeSumTag(99u)

        assertFailsWith<IllegalStateException> {
            ProcedureStatus.decode(BsatnReader(writer.toByteArray()))
        }
    }

    // ---- DecompressedPayload offset validation ----

    @Test
    fun `decompressed payload valid offset`() {
        val data = byteArrayOf(1, 2, 3, 4)
        val payload = DecompressedPayload(data, 1)
        assertEquals(3, payload.size)
    }

    @Test
    fun `decompressed payload zero offset`() {
        val data = byteArrayOf(1, 2, 3)
        val payload = DecompressedPayload(data, 0)
        assertEquals(3, payload.size)
    }

    @Test
    fun `decompressed payload offset at end`() {
        val data = byteArrayOf(1, 2)
        val payload = DecompressedPayload(data, 2)
        assertEquals(0, payload.size)
    }

    @Test
    fun `decompressed payload negative offset rejects`() {
        assertFailsWith<IllegalArgumentException> {
            DecompressedPayload(byteArrayOf(1, 2), -1)
        }
    }

    @Test
    fun `decompressed payload offset beyond size rejects`() {
        assertFailsWith<IllegalArgumentException> {
            DecompressedPayload(byteArrayOf(1, 2), 3)
        }
    }
}
