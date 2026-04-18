package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SqlLit
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import module_bindings.QueryBuilder
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Query builder SQL generation edge cases not already in
 * TypeSafeQueryTest, ColComparisonTest, JoinTest.
 */
class QueryBuilderEdgeCaseTest {

    // --- NOT expression ---

    @Test
    fun `NOT wraps expression in parentheses`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.id.gt(SqlLit.ulong(18UL)).not()
        }.toSql()
        assertTrue(sql.contains("NOT"), "Should contain NOT: $sql")
        assertTrue(sql.contains("(NOT"), "NOT should be parenthesized: $sql")
    }

    // --- NOT with AND ---

    @Test
    fun `NOT combined with AND`() {
        val qb = QueryBuilder()
        val sql = qb.user().where { c ->
            c.online.eq(SqlLit.bool(true)).not()
                .and(c.name.eq(SqlLit.string("admin")))
        }.toSql()
        assertTrue(sql.contains("NOT"), "Should contain NOT: $sql")
        assertTrue(sql.contains("AND"), "Should contain AND: $sql")
    }

    // --- Method-style .and() / .or() chaining ---

    @Test
    fun `method-style and chaining`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.id.gt(SqlLit.ulong(20UL))
                .and(c.id.lt(SqlLit.ulong(30UL)))
        }.toSql()
        assertTrue(sql.contains("> 20"), "Should contain > 20: $sql")
        assertTrue(sql.contains("AND"), "Should contain AND: $sql")
        assertTrue(sql.contains("< 30"), "Should contain < 30: $sql")
    }

    @Test
    fun `method-style or chaining`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.tag.eq(SqlLit.string("work"))
                .or(c.tag.eq(SqlLit.string("personal")))
        }.toSql()
        assertTrue(sql.contains("OR"), "Should contain OR: $sql")
        assertTrue(sql.contains("'work'"), "Should contain 'work': $sql")
        assertTrue(sql.contains("'personal'"), "Should contain 'personal': $sql")
    }

    @Test
    fun `nested and-or-not produces correct structure`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.tag.eq(SqlLit.string("a"))
                .and(c.content.eq(SqlLit.string("b")).or(c.content.eq(SqlLit.string("c"))))
        }.toSql()
        assertTrue(sql.contains("AND"), "Should contain AND: $sql")
        assertTrue(sql.contains("OR"), "Should contain OR: $sql")
    }

    // --- String escaping in WHERE ---

    @Test
    fun `string with single quotes is escaped in WHERE`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.content.eq(SqlLit.string("O'Reilly"))
        }.toSql()
        assertTrue(sql.contains("O''Reilly"), "Single quote should be escaped: $sql")
    }

    @Test
    fun `string with multiple single quotes`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.content.eq(SqlLit.string("it's Bob's"))
        }.toSql()
        assertTrue(sql.contains("it''s Bob''s"), "All single quotes escaped: $sql")
    }

    // --- Bool formatting ---

    @Test
    fun `bool true formats as TRUE`() {
        val qb = QueryBuilder()
        val sql = qb.user().where { c ->
            c.online.eq(SqlLit.bool(true))
        }.toSql()
        assertTrue(sql.contains("TRUE"), "Should contain TRUE: $sql")
    }

    @Test
    fun `bool false formats as FALSE`() {
        val qb = QueryBuilder()
        val sql = qb.user().where { c ->
            c.online.eq(SqlLit.bool(false))
        }.toSql()
        assertTrue(sql.contains("FALSE"), "Should contain FALSE: $sql")
    }

    // --- Identity hex literal in WHERE ---

    @Test
    fun `Identity formats as hex literal in WHERE`() {
        val id = Identity.fromHexString("ab".repeat(32))
        val qb = QueryBuilder()
        val sql = qb.user().where { c ->
            c.identity.eq(SqlLit.identity(id))
        }.toSql()
        assertTrue(sql.contains("0x"), "Identity should be hex literal: $sql")
        assertTrue(sql.contains("ab".repeat(32)), "Should contain hex value: $sql")
    }

    // --- IxCol eq/neq formatting ---

    @Test
    fun `IxCol eq generates correct SQL`() {
        val qb = QueryBuilder()
        val sql = qb.user().where { c ->
            c.identity.eq(SqlLit.identity(Identity.zero()))
        }.toSql()
        assertTrue(sql.contains("\"identity\""), "Should reference identity column: $sql")
        assertTrue(sql.contains("="), "Should contain = operator: $sql")
    }

    @Test
    fun `IxCol neq generates correct SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.id.neq(SqlLit.ulong(0UL))
        }.toSql()
        assertTrue(sql.contains("<>"), "Should contain <> operator: $sql")
    }

    // --- Table scan (no WHERE) produces SELECT * FROM table ---

    @Test
    fun `table scan without where produces simple SELECT`() {
        val qb = QueryBuilder()
        val sql = qb.user().toSql()
        assertEquals("SELECT * FROM \"user\"", sql)
    }

    @Test
    fun `different tables produce different SQL`() {
        val qb = QueryBuilder()
        val userSql = qb.user().toSql()
        val noteSql = qb.note().toSql()
        val messageSql = qb.message().toSql()
        assertTrue(userSql.contains("\"user\""), "Should contain user table: $userSql")
        assertTrue(noteSql.contains("\"note\""), "Should contain note table: $noteSql")
        assertTrue(messageSql.contains("\"message\""), "Should contain message table: $messageSql")
    }

    // --- Column name quoting ---
    // Note: we can't create columns with quotes in our schema, but we can verify
    // that existing column names are properly quoted

    @Test
    fun `column names are double-quoted in SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.tag.eq(SqlLit.string("test"))
        }.toSql()
        assertTrue(sql.contains("\"tag\""), "Column should be double-quoted: $sql")
        assertTrue(sql.contains("\"note\""), "Table should be double-quoted: $sql")
    }

    @Test
    fun `WHERE has table-qualified column names`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.content.eq(SqlLit.string("x"))
        }.toSql()
        assertTrue(sql.contains("\"note\".\"content\""), "Column should be table-qualified: $sql")
    }

    // --- Semijoin with WHERE on both sides ---

    @Test
    fun `left semijoin with where on left table`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .where { c -> c.tag.eq(SqlLit.string("important")) }
            .leftSemijoin(qb.message()) { left, right -> left.id.eq(right.id) }
            .toSql()
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
        assertTrue(sql.contains("\"note\".*"), "Should select note.*: $sql")
        assertTrue(sql.contains("WHERE"), "Should contain WHERE: $sql")
        assertTrue(sql.contains("'important'"), "Should contain left where value: $sql")
    }

    @Test
    fun `right semijoin selects right table columns`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .rightSemijoin(qb.message()) { left, right -> left.id.eq(right.id) }
            .toSql()
        assertTrue(sql.contains("\"message\".*"), "Right semijoin should select message.*: $sql")
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
    }

    @Test
    fun `left semijoin selects left table columns`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .leftSemijoin(qb.message()) { left, right -> left.id.eq(right.id) }
            .toSql()
        assertTrue(sql.contains("\"note\".*"), "Left semijoin should select note.*: $sql")
    }

    // --- Integer formatting ---

    @Test
    fun `integer values format without locale separators`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.id.gt(SqlLit.ulong(1000000UL))
        }.toSql()
        assertTrue(sql.contains("1000000"), "Should not have locale separators: $sql")
        assertTrue(!sql.contains("1,000,000"), "Should not have commas: $sql")
    }

    // --- Empty string literal ---

    @Test
    fun `empty string literal in WHERE`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c ->
            c.tag.eq(SqlLit.string(""))
        }.toSql()
        assertTrue(sql.contains("''"), "Should contain empty string literal: $sql")
    }

    // --- Chained where with filter alias ---

    @Test
    fun `where then filter then where all chain with AND`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .where { c -> c.tag.eq(SqlLit.string("a")) }
            .filter { c -> c.content.eq(SqlLit.string("b")) }
            .where { c -> c.id.gt(SqlLit.ulong(0UL)) }
            .toSql()
        val andCount = Regex("AND").findAll(sql).count()
        assertTrue(andCount >= 2, "Should have at least 2 ANDs: $sql")
    }
}
