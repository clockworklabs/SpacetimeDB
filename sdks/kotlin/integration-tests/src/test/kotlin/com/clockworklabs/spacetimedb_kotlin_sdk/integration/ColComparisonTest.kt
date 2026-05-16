package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.EventContext
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SqlLit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.QueryBuilder
import module_bindings.addQuery
import module_bindings.db
import module_bindings.reducers
import kotlin.test.Test
import kotlin.test.assertTrue

class ColComparisonTest {

    // --- SQL generation tests for lt/lte/gt/gte ---

    @Test
    fun `Col lt generates correct SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c -> c.id.lt(SqlLit.ulong(100uL)) }.toSql()
        assertTrue(sql.contains("< 100"), "Should contain '< 100': $sql")
    }

    @Test
    fun `Col lte generates correct SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c -> c.id.lte(SqlLit.ulong(100uL)) }.toSql()
        assertTrue(sql.contains("<= 100"), "Should contain '<= 100': $sql")
    }

    @Test
    fun `Col gt generates correct SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c -> c.id.gt(SqlLit.ulong(0uL)) }.toSql()
        assertTrue(sql.contains("> 0"), "Should contain '> 0': $sql")
    }

    @Test
    fun `Col gte generates correct SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c -> c.id.gte(SqlLit.ulong(1uL)) }.toSql()
        assertTrue(sql.contains(">= 1"), "Should contain '>= 1': $sql")
    }

    // --- Live subscribe tests ---

    @Test
    fun `gt with live subscribe returns matching rows`() = runBlocking {
        val client = connectToDb()

        // First subscribe to all notes to see what's there
        client.subscribeAll()

        // Insert a note so we have at least one
        val insertDone = CompletableDeferred<ULong>()
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied
                && note.owner == client.identity && note.tag == "gt-test") {
                insertDone.complete(note.id)
            }
        }
        client.conn.reducers.addNote("gt-content", "gt-test")
        val noteId = withTimeout(DEFAULT_TIMEOUT_MS) { insertDone.await() }

        // Now create a second connection with a gt filter
        val client2 = connectToDb()
        val applied = CompletableDeferred<Unit>()

        client2.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery { qb -> qb.note().where { c -> c.id.gte(SqlLit.ulong(noteId)) } }
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        val notes = client2.conn.db.note.all()
        assertTrue(notes.all { it.id >= noteId }, "All notes should have id >= $noteId")

        client2.conn.disconnect()
        client.cleanup()
    }

    // --- Chained where().where() tests ---

    @Test
    fun `chained where produces AND clause`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .where { c -> c.tag.eq(SqlLit.string("test")) }
            .where { c -> c.content.eq(SqlLit.string("hello")) }
            .toSql()
        assertTrue(sql.contains("AND"), "Chained where should produce AND: $sql")
        assertTrue(sql.contains("tag"), "Should contain first where column: $sql")
        assertTrue(sql.contains("content"), "Should contain second where column: $sql")
    }

    @Test
    fun `triple chained where produces two ANDs`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .where { c -> c.tag.eq(SqlLit.string("a")) }
            .where { c -> c.content.eq(SqlLit.string("b")) }
            .where { c -> c.id.gt(SqlLit.ulong(0uL)) }
            .toSql()
        // Count AND occurrences
        val andCount = Regex("AND").findAll(sql).count()
        assertTrue(andCount >= 2, "Triple chain should have >= 2 ANDs, got $andCount: $sql")
    }

    @Test
    fun `chained where with live subscribe works`() = runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        // Insert a note with known tag+content
        val insertDone = CompletableDeferred<Unit>()
        client.conn.db.note.onInsert { ctx, note ->
            if (ctx !is EventContext.SubscribeApplied
                && note.owner == client.identity && note.tag == "chain-test") {
                insertDone.complete(Unit)
            }
        }
        client.conn.reducers.addNote("chain-content", "chain-test")
        withTimeout(DEFAULT_TIMEOUT_MS) { insertDone.await() }

        // Second client subscribes with chained where
        val client2 = connectToDb()
        val applied = CompletableDeferred<Unit>()

        client2.conn.subscriptionBuilder()
            .onApplied { _ -> applied.complete(Unit) }
            .onError { _, err -> applied.completeExceptionally(RuntimeException("$err")) }
            .addQuery { qb ->
                qb.note()
                    .where { c -> c.tag.eq(SqlLit.string("chain-test")) }
                    .where { c -> c.content.eq(SqlLit.string("chain-content")) }
            }
            .subscribe()

        withTimeout(DEFAULT_TIMEOUT_MS) { applied.await() }

        val notes = client2.conn.db.note.all()
        assertTrue(notes.isNotEmpty(), "Should have at least one note matching both where clauses")
        assertTrue(notes.all { it.tag == "chain-test" && it.content == "chain-content" },
            "All notes should match both conditions")

        client2.conn.disconnect()
        client.cleanup()
    }

    // --- Col.eq with another Col (self-join condition) ---

    @Test
    fun `Col eq with another Col generates column comparison SQL`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c -> c.tag.eq(c.content) }.toSql()
        assertTrue(sql.contains("\"tag\"") && sql.contains("\"content\""), "Should reference both columns: $sql")
        assertTrue(sql.contains("="), "Should have = operator: $sql")
    }

    // --- filter alias on FromWhere ---

    @Test
    fun `filter on FromWhere chains like where`() {
        val qb = QueryBuilder()
        val sql = qb.note()
            .where { c -> c.tag.eq(SqlLit.string("a")) }
            .filter { c -> c.content.eq(SqlLit.string("b")) }
            .toSql()
        assertTrue(sql.contains("AND"), "filter after where should also AND: $sql")
    }
}
