package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlin.test.AfterTest

class LoggerTest {
    private val originalLevel = Logger.level
    private val originalHandler = Logger.handler

    @AfterTest
    fun restoreLogger() {
        Logger.level = originalLevel
        Logger.handler = originalHandler
    }

    // ---- Redaction ----

    @Test
    fun `redacts token equals`() {
        val messages = mutableListOf<String>()
        Logger.level = LogLevel.INFO
        Logger.handler = LogHandler { _, msg -> messages.add(msg) }

        Logger.info { "Connecting with token=secret123 to server" }

        assertEquals(1, messages.size)
        assertTrue(messages[0].contains("[REDACTED]"), "Token value should be redacted")
        assertFalse(messages[0].contains("secret123"), "Original secret should not appear")
    }

    @Test
    fun `redacts token colon`() {
        val messages = mutableListOf<String>()
        Logger.level = LogLevel.INFO
        Logger.handler = LogHandler { _, msg -> messages.add(msg) }

        Logger.info { "token: mySecretValue" }

        assertTrue(messages[0].contains("[REDACTED]"))
        assertFalse(messages[0].contains("mySecretValue"))
    }

    @Test
    fun `redacts case insensitive`() {
        val messages = mutableListOf<String>()
        Logger.level = LogLevel.INFO
        Logger.handler = LogHandler { _, msg -> messages.add(msg) }

        Logger.info { "TOKEN=abc123" }
        Logger.info { "Token=def456" }
        Logger.info { "PASSWORD=hunter2" }

        assertEquals(3, messages.size)
        for (msg in messages) {
            assertTrue(msg.contains("[REDACTED]"), "Should redact: $msg")
        }
    }

    @Test
    fun `redacts multiple patterns in one message`() {
        val messages = mutableListOf<String>()
        Logger.level = LogLevel.INFO
        Logger.handler = LogHandler { _, msg -> messages.add(msg) }

        Logger.info { "token=abc password=xyz" }

        assertEquals(1, messages.size)
        assertFalse(messages[0].contains("abc"), "First secret should be redacted")
        assertFalse(messages[0].contains("xyz"), "Second secret should be redacted")
    }

    @Test
    fun `non sensitive passes through`() {
        val messages = mutableListOf<String>()
        Logger.level = LogLevel.INFO
        Logger.handler = LogHandler { _, msg -> messages.add(msg) }

        Logger.info { "Connected to database on port 3000" }

        assertEquals(1, messages.size)
        assertEquals("Connected to database on port 3000", messages[0])
    }

    // ---- Log level filtering ----

    @Test
    fun `should log ordinal logic`() {
        // EXCEPTION(0) should log at any level
        assertTrue(LogLevel.EXCEPTION.shouldLog(LogLevel.EXCEPTION))
        assertTrue(LogLevel.EXCEPTION.shouldLog(LogLevel.TRACE))

        // TRACE(5) should only log at TRACE level
        assertTrue(LogLevel.TRACE.shouldLog(LogLevel.TRACE))
        assertFalse(LogLevel.TRACE.shouldLog(LogLevel.INFO))
        assertFalse(LogLevel.TRACE.shouldLog(LogLevel.EXCEPTION))
    }

    @Test
    fun `log level filters suppresses lower priority`() {
        val messages = mutableListOf<LogLevel>()
        Logger.level = LogLevel.WARN
        Logger.handler = LogHandler { lvl, _ -> messages.add(lvl) }

        Logger.error { "error" }   // should log (ERROR < WARN in ordinal)
        Logger.warn { "warn" }     // should log (WARN == WARN)
        Logger.info { "info" }     // should NOT log (INFO > WARN in ordinal)
        Logger.debug { "debug" }   // should NOT log
        Logger.trace { "trace" }   // should NOT log

        assertEquals(listOf(LogLevel.ERROR, LogLevel.WARN), messages)
    }

    // ---- Custom handler ----

    @Test
    fun `custom handler receives correct level and message`() {
        var capturedLevel: LogLevel? = null
        var capturedMessage: String? = null
        Logger.level = LogLevel.DEBUG
        Logger.handler = LogHandler { lvl, msg ->
            capturedLevel = lvl
            capturedMessage = msg
        }

        Logger.debug { "test message" }

        assertEquals(LogLevel.DEBUG, capturedLevel)
        assertEquals("test message", capturedMessage)
    }
}
