package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import module_bindings.QueryBuilder
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ColExtensionsTest {

    // Test that convenience extensions produce the same SQL as explicit SqlLit calls

    // --- String extensions ---

    @Test
    fun `String eq extension matches SqlLit eq`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.tag.eq("hello") }.toSql()
        val withLit = qb.note().where { c -> c.tag.eq(SqlLit.string("hello")) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `String neq extension matches SqlLit neq`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.tag.neq("hello") }.toSql()
        val withLit = qb.note().where { c -> c.tag.neq(SqlLit.string("hello")) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `String lt extension matches SqlLit lt`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.tag.lt("z") }.toSql()
        val withLit = qb.note().where { c -> c.tag.lt(SqlLit.string("z")) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `String lte extension`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.tag.lte("z") }.toSql()
        val withLit = qb.note().where { c -> c.tag.lte(SqlLit.string("z")) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `String gt extension matches SqlLit gt`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.tag.gt("a") }.toSql()
        val withLit = qb.note().where { c -> c.tag.gt(SqlLit.string("a")) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `String gte extension`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.tag.gte("a") }.toSql()
        val withLit = qb.note().where { c -> c.tag.gte(SqlLit.string("a")) }.toSql()
        assertEquals(withLit, withExt)
    }

    // --- Boolean extensions ---

    @Test
    fun `Boolean eq extension matches SqlLit eq`() {
        val qb = QueryBuilder()
        val withExt = qb.user().where { c -> c.online.eq(true) }.toSql()
        val withLit = qb.user().where { c -> c.online.eq(SqlLit.bool(true)) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `Boolean neq extension matches SqlLit neq`() {
        val qb = QueryBuilder()
        val withExt = qb.user().where { c -> c.online.neq(false) }.toSql()
        val withLit = qb.user().where { c -> c.online.neq(SqlLit.bool(false)) }.toSql()
        assertEquals(withLit, withExt)
    }

    // --- NullableCol String extensions ---

    @Test
    fun `NullableCol String eq extension`() {
        val qb = QueryBuilder()
        val withExt = qb.user().where { c -> c.name.eq("alice") }.toSql()
        val withLit = qb.user().where { c -> c.name.eq(SqlLit.string("alice")) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `NullableCol String gte extension`() {
        val qb = QueryBuilder()
        val withExt = qb.user().where { c -> c.name.gte("a") }.toSql()
        val withLit = qb.user().where { c -> c.name.gte(SqlLit.string("a")) }.toSql()
        assertEquals(withLit, withExt)
    }

    // --- ULong extensions (note.id is Col<Note, ULong>) ---

    @Test
    fun `ULong eq extension`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.id.eq(42uL) }.toSql()
        val withLit = qb.note().where { c -> c.id.eq(SqlLit.ulong(42uL)) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `ULong lt extension`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.id.lt(100uL) }.toSql()
        val withLit = qb.note().where { c -> c.id.lt(SqlLit.ulong(100uL)) }.toSql()
        assertEquals(withLit, withExt)
    }

    @Test
    fun `ULong gte extension`() {
        val qb = QueryBuilder()
        val withExt = qb.note().where { c -> c.id.gte(1uL) }.toSql()
        val withLit = qb.note().where { c -> c.id.gte(SqlLit.ulong(1uL)) }.toSql()
        assertEquals(withLit, withExt)
    }

    // --- IxCol Identity extension (user identity is IxCol) ---

    @Test
    fun `IxCol Identity eq extension`() {
        val qb = QueryBuilder()
        val id = Identity.zero()
        val withExt = qb.user().where { _, ix -> ix.identity.eq(id) }.toSql()
        val withLit = qb.user().where { _, ix -> ix.identity.eq(SqlLit.identity(id)) }.toSql()
        assertEquals(withLit, withExt)
    }

    // --- Verify convenience extensions produce valid SQL ---

    @Test
    fun `convenience extensions produce valid SQL structure`() {
        val qb = QueryBuilder()
        val sql = qb.note().where { c -> c.tag.eq("test") }.toSql()
        assertTrue(sql.contains("SELECT"), "Should be a SELECT: $sql")
        assertTrue(sql.contains("WHERE"), "Should have WHERE: $sql")
        assertTrue(sql.contains("'test'"), "Should contain quoted value: $sql")
    }
}
