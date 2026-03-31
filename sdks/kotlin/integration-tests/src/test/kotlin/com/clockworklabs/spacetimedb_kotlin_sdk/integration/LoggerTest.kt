package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.LogLevel
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Logger
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class LoggerTest {

    private val originalLevel = Logger.level
    private val originalHandler = Logger.handler

    @AfterTest
    fun restore() {
        Logger.level = originalLevel
        Logger.handler = originalHandler
    }

    @Test
    fun `level can be get and set`() {
        Logger.level = LogLevel.DEBUG
        assertEquals(LogLevel.DEBUG, Logger.level)

        Logger.level = LogLevel.ERROR
        assertEquals(LogLevel.ERROR, Logger.level)
    }

    @Test
    fun `custom handler receives log messages`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.TRACE

        Logger.info { "test-info-message" }
        Logger.warn { "test-warn-message" }
        Logger.debug { "test-debug-message" }

        assertTrue(logs.any { it.first == LogLevel.INFO && it.second.contains("test-info-message") })
        assertTrue(logs.any { it.first == LogLevel.WARN && it.second.contains("test-warn-message") })
        assertTrue(logs.any { it.first == LogLevel.DEBUG && it.second.contains("test-debug-message") })
    }

    @Test
    fun `level filters messages below threshold`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.WARN

        Logger.info { "should-be-filtered" }
        Logger.debug { "should-be-filtered" }
        Logger.trace { "should-be-filtered" }
        Logger.warn { "should-appear" }
        Logger.error { "should-appear" }

        assertEquals(2, logs.size, "Only WARN and ERROR should pass, got: $logs")
        assertTrue(logs.all { it.first == LogLevel.WARN || it.first == LogLevel.ERROR })
    }

    @Test
    fun `trace messages pass at TRACE level`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.TRACE

        Logger.trace { "trace-message" }
        assertTrue(logs.any { it.first == LogLevel.TRACE && it.second.contains("trace-message") })
    }

    @Test
    fun `exception with Throwable logs stack trace`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.TRACE

        val ex = RuntimeException("test-exception-message")
        Logger.exception(ex)

        assertTrue(logs.any { it.first == LogLevel.EXCEPTION }, "Should log at EXCEPTION level")
        assertTrue(
            logs.any { it.second.contains("test-exception-message") },
            "Should contain exception message in stack trace"
        )
    }

    @Test
    fun `exception with lambda logs message`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.TRACE

        Logger.exception { "exception-lambda-message" }

        assertTrue(logs.any { it.first == LogLevel.EXCEPTION && it.second.contains("exception-lambda-message") })
    }

    @Test
    fun `sensitive data is redacted`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.TRACE

        Logger.info { "token=my-secret-token-123" }

        val message = logs.first().second
        assertTrue(message.contains("[REDACTED]"), "Token value should be redacted: $message")
        assertTrue(!message.contains("my-secret-token-123"), "Actual token should not appear: $message")
    }

    @Test
    fun `sensitive data redaction covers multiple patterns`() {
        val logs = mutableListOf<Pair<LogLevel, String>>()
        Logger.handler = { level, message -> logs.add(level to message) }
        Logger.level = LogLevel.TRACE

        Logger.info { "password=hunter2 secret=abc123" }

        val message = logs.first().second
        assertTrue(!message.contains("hunter2"), "Password should be redacted: $message")
        assertTrue(!message.contains("abc123"), "Secret should be redacted: $message")
    }

    @Test
    fun `lazy message is not evaluated when level is filtered`() {
        Logger.handler = { _, _ -> }
        Logger.level = LogLevel.ERROR

        var evaluated = false
        Logger.debug { evaluated = true; "should-not-evaluate" }

        assertTrue(!evaluated, "Debug message lambda should not be evaluated at ERROR level")
    }
}
