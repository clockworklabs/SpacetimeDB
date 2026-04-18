package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SqlLit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.QueryBuilder
import module_bindings.addQuery
import module_bindings.db
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class TypeSafeQueryTest {

    @Test
    fun `where with eq generates correct SQL and subscribes`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        // Subscribe using type-safe query: user where online = true
        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery { qb -> qb.user().where { c -> c.online.eq(SqlLit.bool(true)) } }
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        // We should see at least ourselves (we're online)
        val users = client.conn.db.user.all()
        assertTrue(users.isNotEmpty(), "Should see online users")
        assertTrue(users.all { it.online }, "All users should be online with this filter")

        client.conn.disconnect()
    }

    @Test
    fun `filter is alias for where`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery { qb -> qb.user().filter { c -> c.online.eq(SqlLit.bool(true)) } }
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        val users = client.conn.db.user.all()
        assertTrue(users.isNotEmpty(), "Filter should work like where")

        client.conn.disconnect()
    }

    @Test
    fun `neq comparison works`() = runBlocking {
        val client = connectToDb()
        val applied = CompletableDeferred<Unit>()

        // Subscribe to users where online != false (i.e. online users)
        client.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery { qb -> qb.user().where { c -> c.online.neq(SqlLit.bool(false)) } }
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }
        val users = client.conn.db.user.all()
        assertTrue(users.all { it.online }, "neq(false) should return only online users")

        client.conn.disconnect()
    }

    @Test
    fun `boolean combinators and-or-not`() = runBlocking {
        val client = connectToDb()

        // First subscribe to everything so we have data
        client.subscribeAll()

        // Add a note so we can test with note table
        val noteDone = CompletableDeferred<Unit>()
        client.conn.reducers.onAddNote { ctx, _, _ ->
            if (ctx.callerIdentity == client.identity) noteDone.complete(Unit)
        }
        client.conn.reducers.addNote("bool-test-content", "bool-test")
        withTimeout(DEFAULT_TIMEOUT_MS) { noteDone.await() }

        // Test that the query DSL generates valid SQL with and/or/not
        val qb = QueryBuilder()
        val query = qb.note().where { c ->
            c.tag.eq(SqlLit.string("bool-test"))
                .and(c.content.eq(SqlLit.string("bool-test-content")))
        }
        val sql = query.toSql()
        assertTrue(sql.contains("AND"), "SQL should contain AND: $sql")

        val queryOr = qb.note().where { c ->
            c.tag.eq(SqlLit.string("a")).or(c.tag.eq(SqlLit.string("b")))
        }
        assertTrue(queryOr.toSql().contains("OR"), "SQL should contain OR")

        val queryNot = qb.user().where { c ->
            c.online.eq(SqlLit.bool(true)).not()
        }
        assertTrue(queryNot.toSql().contains("NOT"), "SQL should contain NOT")

        client.cleanup()
    }

    @Test
    fun `SqlLit creates typed literals`() = runBlocking {
        // Test various SqlLit factory methods produce valid SQL strings
        assertTrue(SqlLit.string("hello").sql.contains("hello"))
        assertEquals(SqlLit.bool(true).sql, "TRUE")
        assertEquals(SqlLit.bool(false).sql, "FALSE")
        assertEquals(SqlLit.int(42).sql, "42")
        assertEquals(SqlLit.ulong(100UL).sql, "100")
        assertEquals(SqlLit.long(999L).sql, "999")
        assertEquals(SqlLit.float(1.5f).sql, "1.5")
        assertEquals(SqlLit.double(2.5).sql, "2.5")
    }

    @Test
    fun `NullableCol generates valid SQL`() = runBlocking {
        // User.name is a NullableCol<User, String>
        // Test that NullableCol methods produce correct SQL strings
        val qb = QueryBuilder()

        // NullableCol.eq with SqlLiteral
        val eqSql = qb.user().where { c -> c.name.eq(SqlLit.string("alice")) }.toSql()
        assertTrue(eqSql.contains("\"name\"") && eqSql.contains("alice"), "eq SQL: $eqSql")

        // NullableCol.neq with SqlLiteral
        val neqSql = qb.user().where { c -> c.name.neq(SqlLit.string("bob")) }.toSql()
        assertTrue(neqSql.contains("<>"), "neq SQL: $neqSql")

        // NullableCol.eq with another NullableCol (self-reference — valid SQL structure)
        val colEqSql = qb.user().where { c -> c.name.eq(c.name) }.toSql()
        assertTrue(colEqSql.contains("\"name\" = "), "col-eq SQL: $colEqSql")

        // NullableCol comparison operators
        val ltSql = qb.user().where { c -> c.name.lt(SqlLit.string("z")) }.toSql()
        assertTrue(ltSql.contains("<"), "lt SQL: $ltSql")

        val gteSql = qb.user().where { c -> c.name.gte(SqlLit.string("a")) }.toSql()
        assertTrue(gteSql.contains(">="), "gte SQL: $gteSql")
    }
}
