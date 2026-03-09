package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals

class QueryBuilderTest {

    // ---- SqlFormat ----

    @Test
    fun quoteIdentSimple() {
        assertEquals("\"players\"", SqlFormat.quoteIdent("players"))
    }

    @Test
    fun quoteIdentEscapesDoubleQuotes() {
        assertEquals("\"my\"\"table\"", SqlFormat.quoteIdent("my\"table"))
    }

    @Test
    fun formatStringLiteralSimple() {
        assertEquals("'hello'", SqlFormat.formatStringLiteral("hello"))
    }

    @Test
    fun formatStringLiteralEscapesSingleQuotes() {
        assertEquals("'it''s'", SqlFormat.formatStringLiteral("it's"))
    }

    @Test
    fun formatHexLiteralStrips0xPrefix() {
        assertEquals("0xABCD", SqlFormat.formatHexLiteral("0xABCD"))
    }

    @Test
    fun formatHexLiteralWithoutPrefix() {
        assertEquals("0xABCD", SqlFormat.formatHexLiteral("ABCD"))
    }

    // ---- BoolExpr ----

    @Test
    fun boolExprAnd() {
        val a = BoolExpr<Unit>("a = 1")
        val b = BoolExpr<Unit>("b = 2")
        assertEquals("(a = 1 AND b = 2)", a.and(b).sql)
    }

    @Test
    fun boolExprOr() {
        val a = BoolExpr<Unit>("a = 1")
        val b = BoolExpr<Unit>("b = 2")
        assertEquals("(a = 1 OR b = 2)", a.or(b).sql)
    }

    @Test
    fun boolExprNot() {
        val a = BoolExpr<Unit>("x > 5")
        assertEquals("(NOT x > 5)", a.not().sql)
    }

    // ---- Col comparisons ----

    @Test
    fun colEqLiteral() {
        val col = Col<Unit, Int>("t", "x")
        assertEquals("(\"t\".\"x\" = 42)", col.eq(SqlLiteral("42")).sql)
    }

    @Test
    fun colEqOtherCol() {
        val a = Col<Unit, Int>("t", "x")
        val b = Col<Unit, Int>("t", "y")
        assertEquals("(\"t\".\"x\" = \"t\".\"y\")", a.eq(b).sql)
    }

    @Test
    fun colNeq() {
        val col = Col<Unit, String>("t", "name")
        assertEquals("(\"t\".\"name\" <> 'alice')", col.neq(SqlLit.string("alice")).sql)
    }

    @Test
    fun colLtLteGtGte() {
        val col = Col<Unit, Int>("t", "score")
        assertEquals("(\"t\".\"score\" < 10)", col.lt(SqlLit.int(10)).sql)
        assertEquals("(\"t\".\"score\" <= 10)", col.lte(SqlLit.int(10)).sql)
        assertEquals("(\"t\".\"score\" > 10)", col.gt(SqlLit.int(10)).sql)
        assertEquals("(\"t\".\"score\" >= 10)", col.gte(SqlLit.int(10)).sql)
    }

    // ---- Col convenience extensions ----

    @Test
    fun colEqRawInt() {
        val col = Col<Unit, Int>("t", "x")
        assertEquals("(\"t\".\"x\" = 42)", col.eq(42).sql)
    }

    @Test
    fun colEqRawString() {
        val col = Col<Unit, String>("t", "name")
        assertEquals("(\"t\".\"name\" = 'bob')", col.eq("bob").sql)
    }

    @Test
    fun colEqRawBool() {
        val col = Col<Unit, Boolean>("t", "active")
        assertEquals("(\"t\".\"active\" = TRUE)", col.eq(true).sql)
    }

    // ---- IxCol join equality ----

    @Test
    fun ixColJoinEq() {
        val left = IxCol<Unit, Int>("l", "id")
        val right = IxCol<Unit, Int>("r", "lid")
        val join = left.eq(right)
        assertEquals("\"l\".\"id\"", join.leftRefSql)
        assertEquals("\"r\".\"lid\"", join.rightRefSql)
    }

    // ---- Table.toSql ----

    @Test
    fun tableToSql() {
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
    fun tableWhereBoolCol() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> c.active }
        assertEquals("SELECT * FROM \"player\" WHERE (\"player\".\"active\" = TRUE)", q.toSql())
    }

    @Test
    fun tableWhereNotBoolCol() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> !c.active }
        assertEquals("SELECT * FROM \"player\" WHERE (NOT (\"player\".\"active\" = TRUE))", q.toSql())
    }

    @Test
    fun tableWhereToSql() {
        val t = Table<FakeRow, FakeCols, Unit>("player", FakeCols("player"), Unit)
        val q = t.where { c -> c.health.gt(50) }
        assertEquals("SELECT * FROM \"player\" WHERE (\"player\".\"health\" > 50)", q.toSql())
    }

    @Test
    fun fromWhereChainedAnd() {
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
    }
    class RightIxCols(tableName: String) {
        val lid = IxCol<RightRow, Int>(tableName, "lid")
    }

    @Test
    fun leftSemiJoinToSql() {
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
    fun rightSemiJoinToSql() {
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
    fun fromWhereLeftSemiJoinToSql() {
        val left = Table<LeftRow, LeftCols, LeftIxCols>("a", LeftCols("a"), LeftIxCols("a"))
        val right = Table<RightRow, Unit, RightIxCols>("b", Unit, RightIxCols("b"))
        val q = left.where { c: LeftCols -> c.status.eq("active") }
            .leftSemijoin(right) { l, r -> l.id.eq(r.lid) }
        assertEquals(
            "SELECT \"a\".* FROM \"a\" JOIN \"b\" ON \"a\".\"id\" = \"b\".\"lid\" WHERE (\"a\".\"status\" = 'active')",
            q.toSql()
        )
    }

    // ---- SqlLit factory methods ----

    @Test
    fun sqlLitBool() {
        assertEquals("TRUE", SqlLit.bool(true).sql)
        assertEquals("FALSE", SqlLit.bool(false).sql)
    }

    @Test
    fun sqlLitNumericTypes() {
        assertEquals("42", SqlLit.int(42).sql)
        assertEquals("100", SqlLit.long(100L).sql)
        assertEquals("7", SqlLit.byte(7).sql)
        assertEquals("1000", SqlLit.short(1000).sql)
        assertEquals("3.14", SqlLit.float(3.14f).sql)
        assertEquals("2.718", SqlLit.double(2.718).sql)
    }

    @Test
    fun sqlLitString() {
        assertEquals("'hello world'", SqlLit.string("hello world").sql)
    }
}
