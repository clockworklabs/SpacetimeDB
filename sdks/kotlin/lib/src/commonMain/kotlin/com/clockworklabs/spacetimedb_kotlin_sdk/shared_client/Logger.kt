@file:Suppress("unused")

package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * Log levels matching C#'s ISpacetimeDBLogger / TS's stdbLogger.
 */
enum class LogLevel {
    EXCEPTION, ERROR, WARN, INFO, DEBUG, TRACE;

    fun shouldLog(threshold: LogLevel): Boolean = this.ordinal <= threshold.ordinal
}

/**
 * Handler for log output. Implement to route logs to a custom destination.
 */
fun interface LogHandler {
    fun log(level: LogLevel, message: String)
}

private val SENSITIVE_KEYS = setOf("token", "authToken", "auth_token", "password", "secret", "credential")

/**
 * Redact sensitive key-value pairs from a message string.
 */
private fun redactSensitive(message: String): String {
    var result = message
    for (key in SENSITIVE_KEYS) {
        result = result.replace(Regex("""($key\s*[=:]\s*)\S+""", RegexOption.IGNORE_CASE), "$1[REDACTED]")
    }
    return result
}

/**
 * Global logger for the SpacetimeDB SDK.
 * Configurable level and handler with lazy message evaluation.
 */
object Logger {
    var level: LogLevel = LogLevel.INFO
    var handler: LogHandler = LogHandler { lvl, msg ->
        println("[SpacetimeDB ${lvl.name}] $msg")
    }

    fun exception(throwable: Throwable) {
        if (LogLevel.EXCEPTION.shouldLog(level)) handler.log(LogLevel.EXCEPTION, throwable.stackTraceToString())
    }

    fun exception(message: () -> String) {
        if (LogLevel.EXCEPTION.shouldLog(level)) handler.log(LogLevel.EXCEPTION, redactSensitive(message()))
    }

    fun error(message: () -> String) {
        if (LogLevel.ERROR.shouldLog(level)) handler.log(LogLevel.ERROR, redactSensitive(message()))
    }

    fun warn(message: () -> String) {
        if (LogLevel.WARN.shouldLog(level)) handler.log(LogLevel.WARN, redactSensitive(message()))
    }

    fun info(message: () -> String) {
        if (LogLevel.INFO.shouldLog(level)) handler.log(LogLevel.INFO, redactSensitive(message()))
    }

    fun debug(message: () -> String) {
        if (LogLevel.DEBUG.shouldLog(level)) handler.log(LogLevel.DEBUG, redactSensitive(message()))
    }

    fun trace(message: () -> String) {
        if (LogLevel.TRACE.shouldLog(level)) handler.log(LogLevel.TRACE, redactSensitive(message()))
    }
}
