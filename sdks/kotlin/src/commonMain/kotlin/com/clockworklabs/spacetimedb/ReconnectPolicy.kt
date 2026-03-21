package com.clockworklabs.spacetimedb

/**
 * Configures automatic reconnection with exponential backoff.
 *
 * @property maxRetries Maximum number of reconnect attempts before giving up.
 * @property initialDelayMs Delay before the first retry (milliseconds).
 * @property maxDelayMs Upper bound on the delay between retries (milliseconds).
 * @property backoffMultiplier Factor by which the delay grows each attempt.
 */
data class ReconnectPolicy(
    val maxRetries: Int = 5,
    val initialDelayMs: Long = 1_000,
    val maxDelayMs: Long = 30_000,
    val backoffMultiplier: Double = 2.0,
) {
    init {
        require(maxRetries >= 0) { "maxRetries must be non-negative" }
        require(initialDelayMs > 0) { "initialDelayMs must be positive" }
        require(maxDelayMs >= initialDelayMs) { "maxDelayMs must be >= initialDelayMs" }
        require(backoffMultiplier >= 1.0) { "backoffMultiplier must be >= 1.0" }
    }

    internal fun delayForAttempt(attempt: Int): Long {
        var delay = initialDelayMs
        repeat(attempt) {
            delay = (delay * backoffMultiplier).toLong().coerceAtMost(maxDelayMs)
        }
        return delay
    }
}
