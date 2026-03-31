package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ScheduleAt
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Timestamp
import module_bindings.Message
import module_bindings.Note
import module_bindings.Reminder
import module_bindings.User
import module_bindings.db
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.minutes

/**
 * Generated data class equality, hashCode, toString, and copy tests.
 */
class GeneratedTypeTest {

    private val identity1 = Identity.fromHexString("aa".repeat(32))
    private val identity2 = Identity.fromHexString("bb".repeat(32))
    private val ts = Timestamp.fromMillis(1700000000000L)

    // --- User equals/hashCode ---

    @Test
    fun `User equals same values`() {
        val a = User(identity1, "Alice", true)
        val b = User(identity1, "Alice", true)
        assertEquals(a, b)
    }

    @Test
    fun `User not equals different identity`() {
        val a = User(identity1, "Alice", true)
        val b = User(identity2, "Alice", true)
        assertNotEquals(a, b)
    }

    @Test
    fun `User not equals different name`() {
        val a = User(identity1, "Alice", true)
        val b = User(identity1, "Bob", true)
        assertNotEquals(a, b)
    }

    @Test
    fun `User not equals different online`() {
        val a = User(identity1, "Alice", true)
        val b = User(identity1, "Alice", false)
        assertNotEquals(a, b)
    }

    @Test
    fun `User equals with null name`() {
        val a = User(identity1, null, false)
        val b = User(identity1, null, false)
        assertEquals(a, b)
    }

    @Test
    fun `User not equals null vs non-null name`() {
        val a = User(identity1, null, true)
        val b = User(identity1, "Alice", true)
        assertNotEquals(a, b)
    }

    @Test
    fun `User hashCode consistent with equals`() {
        val a = User(identity1, "Alice", true)
        val b = User(identity1, "Alice", true)
        assertEquals(a.hashCode(), b.hashCode())
    }

    @Test
    fun `User hashCode differs for different values`() {
        val a = User(identity1, "Alice", true)
        val b = User(identity2, "Bob", false)
        assertNotEquals(a.hashCode(), b.hashCode())
    }

    // --- User toString ---

    @Test
    fun `User toString contains field values`() {
        val user = User(identity1, "Alice", true)
        val str = user.toString()
        assertTrue(str.contains("Alice"), "toString should contain name: $str")
        assertTrue(str.contains("true"), "toString should contain online: $str")
        assertTrue(str.contains("User"), "toString should contain class name: $str")
    }

    @Test
    fun `User toString with null name`() {
        val user = User(identity1, null, false)
        val str = user.toString()
        assertTrue(str.contains("null"), "toString should show null for name: $str")
    }

    // --- Message equals/hashCode ---

    @Test
    fun `Message equals same values`() {
        val a = Message(1UL, identity1, ts, "hello")
        val b = Message(1UL, identity1, ts, "hello")
        assertEquals(a, b)
    }

    @Test
    fun `Message not equals different id`() {
        val a = Message(1UL, identity1, ts, "hello")
        val b = Message(2UL, identity1, ts, "hello")
        assertNotEquals(a, b)
    }

    @Test
    fun `Message not equals different text`() {
        val a = Message(1UL, identity1, ts, "hello")
        val b = Message(1UL, identity1, ts, "world")
        assertNotEquals(a, b)
    }

    @Test
    fun `Message toString contains field values`() {
        val msg = Message(42UL, identity1, ts, "test message")
        val str = msg.toString()
        assertTrue(str.contains("42"), "toString should contain id: $str")
        assertTrue(str.contains("test message"), "toString should contain text: $str")
        assertTrue(str.contains("Message"), "toString should contain class name: $str")
    }

    // --- Note equals/hashCode ---

    @Test
    fun `Note equals same values`() {
        val a = Note(1UL, identity1, "content", "tag")
        val b = Note(1UL, identity1, "content", "tag")
        assertEquals(a, b)
    }

    @Test
    fun `Note not equals different tag`() {
        val a = Note(1UL, identity1, "content", "tag1")
        val b = Note(1UL, identity1, "content", "tag2")
        assertNotEquals(a, b)
    }

    @Test
    fun `Note hashCode consistent with equals`() {
        val a = Note(5UL, identity1, "x", "y")
        val b = Note(5UL, identity1, "x", "y")
        assertEquals(a.hashCode(), b.hashCode())
    }

    // --- Reminder equals/hashCode ---

    @Test
    fun `Reminder equals same values`() {
        val sa = ScheduleAt.interval(5.minutes)
        val a = Reminder(1UL, sa, "remind me", identity1)
        val b = Reminder(1UL, sa, "remind me", identity1)
        assertEquals(a, b)
    }

    @Test
    fun `Reminder not equals different text`() {
        val sa = ScheduleAt.interval(5.minutes)
        val a = Reminder(1UL, sa, "first", identity1)
        val b = Reminder(1UL, sa, "second", identity1)
        assertNotEquals(a, b)
    }

    @Test
    fun `Reminder toString contains field values`() {
        val sa = ScheduleAt.interval(5.minutes)
        val r = Reminder(99UL, sa, "reminder text", identity1)
        val str = r.toString()
        assertTrue(str.contains("99"), "toString should contain scheduledId: $str")
        assertTrue(str.contains("reminder text"), "toString should contain text: $str")
        assertTrue(str.contains("Reminder"), "toString should contain class name: $str")
    }

    // --- Copy (Kotlin data class feature) ---

    @Test
    fun `User copy preserves unchanged fields`() {
        val original = User(identity1, "Alice", true)
        val copy = original.copy(name = "Bob")
        assertEquals(identity1, copy.identity)
        assertEquals("Bob", copy.name)
        assertEquals(true, copy.online)
    }

    @Test
    fun `Message copy with different id`() {
        val original = Message(1UL, identity1, ts, "hello")
        val copy = original.copy(id = 99UL)
        assertEquals(99UL, copy.id)
        assertEquals(identity1, copy.sender)
        assertEquals("hello", copy.text)
    }

    // --- Destructuring (Kotlin data class feature) ---

    @Test
    fun `User destructuring`() {
        val user = User(identity1, "Alice", true)
        val (identity, name, online) = user
        assertEquals(identity1, identity)
        assertEquals("Alice", name)
        assertEquals(true, online)
    }

    @Test
    fun `Note destructuring`() {
        val note = Note(7UL, identity1, "content", "tag")
        val (id, owner, content, tag) = note
        assertEquals(7UL, id)
        assertEquals(identity1, owner)
        assertEquals("content", content)
        assertEquals("tag", tag)
    }

    // --- Live roundtrip through server ---

    @Test
    fun `User from server has correct data class behavior`() = kotlinx.coroutines.runBlocking {
        val client = connectToDb()
        client.subscribeAll()

        val user = client.conn.db.user.identity.find(client.identity)!!

        // data class equals works with server-returned instances
        val userCopy = user.copy()
        assertEquals(user, userCopy)
        assertEquals(user.hashCode(), userCopy.hashCode())

        // toString is meaningful
        val str = user.toString()
        assertTrue(str.contains("User"), "Server user toString: $str")

        client.conn.disconnect()
    }
}
