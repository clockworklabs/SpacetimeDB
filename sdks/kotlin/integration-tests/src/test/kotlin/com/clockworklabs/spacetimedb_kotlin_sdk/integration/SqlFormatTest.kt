package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SqlFormat
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class SqlFormatTest {

    // --- quoteIdent ---

    @Test
    fun `quoteIdent wraps in double quotes`() {
        assertEquals("\"tableName\"", SqlFormat.quoteIdent("tableName"))
    }

    @Test
    fun `quoteIdent escapes internal double quotes`() {
        assertEquals("\"bad\"\"name\"", SqlFormat.quoteIdent("bad\"name"))
    }

    @Test
    fun `quoteIdent handles empty string`() {
        assertEquals("\"\"", SqlFormat.quoteIdent(""))
    }

    @Test
    fun `quoteIdent with multiple double quotes`() {
        assertEquals("\"a\"\"b\"\"c\"", SqlFormat.quoteIdent("a\"b\"c"))
    }

    @Test
    fun `quoteIdent preserves spaces`() {
        assertEquals("\"my table\"", SqlFormat.quoteIdent("my table"))
    }

    // --- formatStringLiteral ---

    @Test
    fun `formatStringLiteral wraps in single quotes`() {
        assertEquals("'hello'", SqlFormat.formatStringLiteral("hello"))
    }

    @Test
    fun `formatStringLiteral escapes single quotes`() {
        assertEquals("'O''Brien'", SqlFormat.formatStringLiteral("O'Brien"))
    }

    @Test
    fun `formatStringLiteral empty string`() {
        assertEquals("''", SqlFormat.formatStringLiteral(""))
    }

    @Test
    fun `formatStringLiteral multiple single quotes`() {
        assertEquals("'it''s a ''test'''", SqlFormat.formatStringLiteral("it's a 'test'"))
    }

    @Test
    fun `formatStringLiteral preserves double quotes`() {
        assertEquals("'say \"hi\"'", SqlFormat.formatStringLiteral("say \"hi\""))
    }

    @Test
    fun `formatStringLiteral preserves special chars`() {
        assertEquals("'tab\tnewline\n'", SqlFormat.formatStringLiteral("tab\tnewline\n"))
    }

    // --- formatHexLiteral ---

    @Test
    fun `formatHexLiteral adds 0x prefix`() {
        assertEquals("0x01020304", SqlFormat.formatHexLiteral("01020304"))
    }

    @Test
    fun `formatHexLiteral strips existing 0x prefix`() {
        assertEquals("0xabcdef", SqlFormat.formatHexLiteral("0xabcdef"))
    }

    @Test
    fun `formatHexLiteral strips 0X prefix case insensitive`() {
        assertEquals("0xABCDEF", SqlFormat.formatHexLiteral("0XABCDEF"))
    }

    @Test
    fun `formatHexLiteral strips hyphens`() {
        assertEquals("0x0123456789ab", SqlFormat.formatHexLiteral("01234567-89ab"))
    }

    @Test
    fun `formatHexLiteral accepts uppercase hex`() {
        assertEquals("0xABCD", SqlFormat.formatHexLiteral("ABCD"))
    }

    @Test
    fun `formatHexLiteral accepts mixed case hex`() {
        assertEquals("0xAbCd", SqlFormat.formatHexLiteral("AbCd"))
    }

    @Test
    fun `formatHexLiteral rejects non-hex chars`() {
        assertFailsWith<IllegalArgumentException> {
            SqlFormat.formatHexLiteral("xyz123")
        }
    }

    @Test
    fun `formatHexLiteral rejects empty after prefix strip`() {
        assertFailsWith<IllegalArgumentException> {
            SqlFormat.formatHexLiteral("0x")
        }
    }

    @Test
    fun `formatHexLiteral rejects empty string`() {
        assertFailsWith<IllegalArgumentException> {
            SqlFormat.formatHexLiteral("")
        }
    }
}
