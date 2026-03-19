package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import kotlinx.atomicfu.atomic

/**
 * Log levels matching C#'s ISpacetimeDBLogger / TS's stdbLogger.
 */
public enum class LogLevel {
    EXCEPTION, ERROR, WARN, INFO, DEBUG, TRACE;

    public fun shouldLog(threshold: LogLevel): Boolean = this.ordinal <= threshold.ordinal
}

/**
 * Handler for log output. Implement to route logs to a custom destination.
 */
public fun interface LogHandler {
    public fun log(level: LogLevel, message: String)
}

private val SENSITIVE_KEYS = listOf("token", "authtoken", "auth_token", "password", "secret", "credential", "api_key", "apikey", "bearer")

private val SENSITIVE_PATTERNS: List<Regex> by lazy {
    SENSITIVE_KEYS.map { key ->
        Regex("""($key\s*[=:]\s*)\S+""", RegexOption.IGNORE_CASE)
    }
}

/**
 * Redact sensitive key-value pairs from a message string.
 */
private fun redactSensitive(message: String): String {
    val lower = message.lowercase()
    if (SENSITIVE_KEYS.none { it in lower }) return message
    var result = message
    for (pattern in SENSITIVE_PATTERNS) {
        result = result.replace(pattern, "$1[REDACTED]")
    }
    return result
}

/**
 * Global logger for the SpacetimeDB SDK.
 * Configurable level and handler with lazy message evaluation.
 */
public object Logger {
    private val _level = atomic(LogLevel.INFO)
    private val _handler = atomic<LogHandler>(LogHandler { lvl, msg ->
        println("[SpacetimeDB ${lvl.name}] $msg")
    })

    public var level: LogLevel
        get() = _level.value
        set(value) { _level.value = value }

    public var handler: LogHandler
        get() = _handler.value
        set(value) { _handler.value = value }

    public fun exception(throwable: Throwable) {
        if (LogLevel.EXCEPTION.shouldLog(level)) handler.log(LogLevel.EXCEPTION, redactSensitive(throwable.stackTraceToString()))
    }

    public fun exception(message: () -> String) {
        if (LogLevel.EXCEPTION.shouldLog(level)) handler.log(LogLevel.EXCEPTION, redactSensitive(message()))
    }

    public fun error(message: () -> String) {
        if (LogLevel.ERROR.shouldLog(level)) handler.log(LogLevel.ERROR, redactSensitive(message()))
    }

    public fun warn(message: () -> String) {
        if (LogLevel.WARN.shouldLog(level)) handler.log(LogLevel.WARN, redactSensitive(message()))
    }

    public fun info(message: () -> String) {
        if (LogLevel.INFO.shouldLog(level)) handler.log(LogLevel.INFO, redactSensitive(message()))
    }

    public fun debug(message: () -> String) {
        if (LogLevel.DEBUG.shouldLog(level)) handler.log(LogLevel.DEBUG, redactSensitive(message()))
    }

    public fun trace(message: () -> String) {
        if (LogLevel.TRACE.shouldLog(level)) handler.log(LogLevel.TRACE, redactSensitive(message()))
    }
}
