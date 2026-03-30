package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse

@OptIn(InternalSpacetimeApi::class)
class QueryBuilderTest {

    // ---- SqlFormat ----

    @Test
    fun `quote ident simple`() {
        assertEquals("\"players\"", SqlFormat.quoteIdent("players"))
    }

    @Test
    fun `quote ident escapes double quotes`() {
        assertEquals("\"my\"\"table\"", SqlFormat.quoteIdent("my\"table"))
    }

    @Test
    fun `format string literal simple`() {
        assertEquals("'hello'", SqlFormat.formatStringLiteral("hello"))
    }

    @Test
    fun `format string literal escapes single quotes`() {
        assertEquals("'it''s'", SqlFormat.formatStringLiteral("it's"))
    }

    @Test
    fun `format hex literal strips 0x prefix`() {
        assertEquals("0xABCD", SqlFormat.formatHexLiteral("0xABCD"))
    }

    @Test
    fun `format hex literal without prefix`() {
        assertEquals("0xABCD", SqlFormat.formatHexLiteral("ABCD"))
    }

    // ---- SqlLit NaN/Infinity rejection ----

    @Test
    fun `float nan throws`() {
        assertFailsWith<IllegalArgumentException> { SqlLit.float(Float.NaN) }
    }

    @Test
    fun `float positive infinity throws`() {
        assertFailsWith<IllegalArgumentException> { SqlLit.float(Float.POSITIVE_INFINITY) }
    }

    @Test
    fun `float negative infinity throws`() {
        assertFailsWith<IllegalArgumentException> { SqlLit.float(Float.NEGATIVE_INFINITY) }
    }

    @Test
    fun `double nan throws`() {
        assertFailsWith<IllegalArgumentException> { SqlLit.double(Double.NaN) }
    }

    @Test
    fun `double positive infinity throws`() {
        assertFailsWith<IllegalArgumentException> { SqlLit.double(Double.POSITIVE_INFINITY) }
    }

    @Test
    fun `double negative infinity throws`() {
        assertFailsWith<IllegalArgumentException> { SqlLit.double(Double.NEGATIVE_INFINITY) }
    }

    @Test
    fun `finite float succeeds`() {
        assertEquals("3.14", SqlLit.float(3.14f).sql)
    }

    @Test
    fun `finite double succeeds`() {
        assertEquals("2.718", SqlLit.double(2.718).sql)
    }

    @Test
    fun `float scientific notation produces plain decimal`() {
        val sql = SqlLit.float(1.0E-7f).sql
        assertFalse(sql.contains("E", ignoreCase = true), "Expected plain decimal, got: $sql")
    }

    @Test
    fun `double scientific notation produces plain decimal`() {
        val sql = SqlLit.double(1.0E-7).sql
        assertFalse(sql.contains("E", ignoreCase = true), "Expected plain decimal, got: $sql")
    }

    // ---- BoolExpr ----

    @Test
    fun `bool expr and`() {
        val a = BoolExpr<Unit>("a = 1")
        val b = BoolExpr<Unit>("b = 2")
        assertEquals("(a = 1 AND b = 2)", a.and(b).sql)
    }

    @Test
    fun `bool expr or`() {
        val a = BoolExpr<Unit>("a = 1")
        val b = BoolExpr<Unit>("b = 2")
        assertEquals("(a = 1 OR b = 2)", a.or(b).sql)
    }

    @Test
    fun `bool expr not`() {
        val a = BoolExpr<Unit>("x > 5")
        assertEquals("(NOT x > 5)", a.not().sql)
    }

    // ---- Col comparisons ----

    @Test
    fun `col eq literal`() {
        val col = Col<Unit, Int>("t", "x")
        assertEquals("(\"t\".\"x\" = 42)", col.eq(SqlLiteral("42")).sql)
    }

    @Test
    fun `col eq other col`() {
        val a = Col<Unit, Int>("t", "x")
        val b = Col<Unit, Int>("t", "y")
        assertEquals("(\"t\".\"x\" = \"t\".\"y\")", a.eq(b).sql)
    }

    @Test
    fun `col neq`() {
        val col = Col<Unit, String>("t", "name")
        assertEquals("(\"t\".\"name\" <> 'alice')", col.neq(SqlLit.string("alice")).sql)
    }

    @Test
    fun `col lt lte gt gte`() {
        val col = Col<Unit, Int>("t", "score")
        assertEquals("(\"t\".\"score\" < 10)", col.lt(SqlLit.int(10)).sql)
        assertEquals("(\"t\".\"score\" <= 10)", col.lte(SqlLit.int(10)).sql)
        assertEquals("(\"t\".\"score\" > 10)", col.gt(SqlLit.int(10)).sql)
        assertEquals("(\"t\".\"score\" >= 10)", col.gte(SqlLit.int(10)).sql)
    }

    // ---- Col convenience extensions ----

    @Test
    fun `col eq raw int`() {
        val col = Col<Unit, Int>("t", "x")
        assertEquals("(\"t\".\"x\" = 42)", col.eq(42).sql)
    }

    @Test
    fun `col eq raw string`() {
        val col = Col<Unit, String>("t", "name")
        assertEquals("(\"t\".\"name\" = 'bob')", col.eq("bob").sql)
    }

    @Test
    fun `col eq raw bool`() {
        val col = Col<Unit, Boolean>("t", "active")
        assertEquals("(\"t\".\"active\" = TRUE)", col.eq(true).sql)
    }

    // ---- IxCol join equality ----

    @Test
    fun `ix col join eq`() {
        val left = IxCol<Unit, Int>("l", "id")
        val right = IxCol<Unit, Int>("r", "lid")
        val join = left.eq(right)
        assertEquals("\"l\".\"id\"", join.leftRefSql)
        assertEquals("\"r\".\"lid\"", join.rightRefSql)
    }

    // ---- Table.toSql ----

    @Test
    fun `table to sql`() {
        val t = Table<Unit, Unit, Unit>("players", Unit, Unit)
        assertEquals("SELECT * FROM \"players\"", t.toSql())
    }

    // ---- Table.where -> FromWhere ----

    data class FakeRow(val x: Int)
    class FakeCols(tableName: String) {
        val health = Col<FakeRow, Int>(tableName, "health")
        val name = Col<FakeRow, String>(tableName, "name")
        val active = Col<FakeRow, Boolean>(tableName, "active")
    }

    @Test
    fun `table where bool col`() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> c.active }
        assertEquals("SELECT * FROM \"player\" WHERE (\"player\".\"active\" = TRUE)", q.toSql())
    }

    @Test
    fun `table where not bool col`() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> !c.active }
        assertEquals("SELECT * FROM \"player\" WHERE (NOT (\"player\".\"active\" = TRUE))", q.toSql())
    }

    @Test
    fun `table where to sql`() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> c.health.gt(50) }
        assertEquals("SELECT * FROM \"player\" WHERE (\"player\".\"health\" > 50)", q.toSql())
    }

    @Test
    fun `from where chained and`() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> c.health.gt(50) }
            .where { c -> c.name.eq("alice") }
        assertEquals(
            "SELECT * FROM \"player\" WHERE ((\"player\".\"health\" > 50) AND (\"player\".\"name\" = 'alice'))",
            q.toSql()
        )
    }

    // ---- LeftSemiJoin ----

    data class LeftRow(val id: Int)
    data class RightRow(val lid: Int)

    class LeftIxCols(tableName: String) {
        val id = IxCol<LeftRow, Int>(tableName, "id")
        val verified = IxCol<LeftRow, Boolean>(tableName, "verified")
    }
    class RightIxCols(tableName: String) {
        val lid = IxCol<RightRow, Int>(tableName, "lid")
    }

    @Test
    fun `left semi join to sql`() {
        val left = Table<LeftRow, Unit, LeftIxCols>("a", Unit, LeftIxCols("a"))
        val right = Table<RightRow, Unit, RightIxCols>("b", Unit, RightIxCols("b"))
        val q = left.leftSemijoin(right) { l, r -> l.id.eq(r.lid) }
        assertEquals(
            "SELECT \"a\".* FROM \"a\" JOIN \"b\" ON \"a\".\"id\" = \"b\".\"lid\"",
            q.toSql()
        )
    }

    // ---- RightSemiJoin ----

    @Test
    fun `right semi join to sql`() {
        val left = Table<LeftRow, Unit, LeftIxCols>("a", Unit, LeftIxCols("a"))
        val right = Table<RightRow, Unit, RightIxCols>("b", Unit, RightIxCols("b"))
        val q = left.rightSemijoin(right) { l, r -> l.id.eq(r.lid) }
        assertEquals(
            "SELECT \"b\".* FROM \"a\" JOIN \"b\" ON \"a\".\"id\" = \"b\".\"lid\"",
            q.toSql()
        )
    }

    // ---- FromWhere -> LeftSemiJoin ----

    class LeftCols(tableName: String) {
        val status = Col<LeftRow, String>(tableName, "status")
    }

    @Test
    fun `from where left semi join to sql`() {
        val left = Table<LeftRow, LeftCols, LeftIxCols>("a", LeftCols("a"), LeftIxCols("a"))
        val right = Table<RightRow, Unit, RightIxCols>("b", Unit, RightIxCols("b"))
        val q = left.where { c: LeftCols -> c.status.eq("active") }
            .leftSemijoin(right) { l, r -> l.id.eq(r.lid) }
        assertEquals(
            "SELECT \"a\".* FROM \"a\" JOIN \"b\" ON \"a\".\"id\" = \"b\".\"lid\" WHERE (\"a\".\"status\" = 'active')",
            q.toSql()
        )
    }

    // ---- where with IxCol<Boolean> ----

    @Test
    fun `table where ix col bool`() {
        val t = Table<LeftRow, LeftCols, LeftIxCols>("a", LeftCols("a"), LeftIxCols("a"))
        val q = t.where { _, ix -> ix.verified }
        assertEquals("SELECT * FROM \"a\" WHERE (\"a\".\"verified\" = TRUE)", q.toSql())
    }

    @Test
    fun `table where not ix col bool`() {
        val t = Table<LeftRow, LeftCols, LeftIxCols>("a", LeftCols("a"), LeftIxCols("a"))
        val q = t.where { _, ix -> !ix.verified }
        assertEquals("SELECT * FROM \"a\" WHERE (NOT (\"a\".\"verified\" = TRUE))", q.toSql())
    }

    // ---- SqlLit factory methods ----

    @Test
    fun `sql lit bool`() {
        assertEquals("TRUE", SqlLit.bool(true).sql)
        assertEquals("FALSE", SqlLit.bool(false).sql)
    }

    @Test
    fun `sql lit numeric types`() {
        assertEquals("42", SqlLit.int(42).sql)
        assertEquals("100", SqlLit.long(100L).sql)
        assertEquals("7", SqlLit.byte(7).sql)
        assertEquals("1000", SqlLit.short(1000).sql)
        assertEquals("3.14", SqlLit.float(3.14f).sql)
        assertEquals("2.718", SqlLit.double(2.718).sql)
    }

    @Test
    fun `sql lit string`() {
        assertEquals("'hello world'", SqlLit.string("hello world").sql)
    }
}
