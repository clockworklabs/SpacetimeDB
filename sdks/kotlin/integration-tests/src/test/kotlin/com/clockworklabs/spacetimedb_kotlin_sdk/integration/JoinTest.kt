package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SqlLit
import kotlinx.coroutines.runBlocking
import module_bindings.QueryBuilder
import kotlin.test.Test
import kotlin.test.assertTrue

class JoinTest {

    @Test
    fun `leftSemijoin generates valid SQL`() = runBlocking {
        val qb = QueryBuilder()
        // note.id JOIN message.id (both IxCol<*, ULong>) — synthetic but tests the API
        val query = qb.note().leftSemijoin(qb.message()) { left, right ->
            left.id.eq(right.id)
        }
        val sql = query.toSql()
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
        assertTrue(sql.contains("\"note\".*"), "Should select note.*: $sql")
        assertTrue(sql.contains("\"note\".\"id\" = \"message\".\"id\""), "Should have ON clause: $sql")
    }

    @Test
    fun `rightSemijoin generates valid SQL`() = runBlocking {
        val qb = QueryBuilder()
        val query = qb.note().rightSemijoin(qb.message()) { left, right ->
            left.id.eq(right.id)
        }
        val sql = query.toSql()
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
        assertTrue(sql.contains("\"message\".*"), "Should select message.*: $sql")
    }

    @Test
    fun `leftSemijoin with where clause`() = runBlocking {
        val qb = QueryBuilder()
        val query = qb.note()
            .leftSemijoin(qb.message()) { left, right -> left.id.eq(right.id) }
            .where { c -> c.tag.eq(SqlLit.string("test")) }
        val sql = query.toSql()
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
        assertTrue(sql.contains("WHERE"), "Should contain WHERE: $sql")
    }

    @Test
    fun `rightSemijoin with where clause`() = runBlocking {
        val qb = QueryBuilder()
        val query = qb.note()
            .rightSemijoin(qb.message()) { left, right -> left.id.eq(right.id) }
            .where { c -> c.text.eq(SqlLit.string("hello")) }
        val sql = query.toSql()
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
        assertTrue(sql.contains("WHERE"), "Should contain WHERE: $sql")
    }

    @Test
    fun `IxCol eq produces IxJoinEq for join condition`() = runBlocking {
        val qb = QueryBuilder()
        // Verify the IxCol.eq(otherIxCol) produces the correct ON clause
        val query = qb.note().leftSemijoin(qb.message()) { left, right ->
            left.id.eq(right.id)
        }
        val sql = query.toSql()
        // The ON clause should reference both table columns
        assertTrue(sql.contains("\"note\".\"id\""), "Should reference note.id: $sql")
        assertTrue(sql.contains("\"message\".\"id\""), "Should reference message.id: $sql")
    }

    @Test
    fun `FromWhere leftSemijoin chains where then join`() = runBlocking {
        val qb = QueryBuilder()
        val query = qb.note()
            .where { c -> c.tag.eq(SqlLit.string("important")) }
            .leftSemijoin(qb.message()) { left, right -> left.id.eq(right.id) }
        val sql = query.toSql()
        assertTrue(sql.contains("JOIN"), "Should contain JOIN: $sql")
        assertTrue(sql.contains("WHERE"), "Should contain WHERE from pre-join filter: $sql")
    }
}
