package com.clockworklabs.spacetimedb

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ReconnectPolicyTest {

    @Test
    fun defaultPolicy() {
        val policy = ReconnectPolicy()
        assertEquals(5, policy.maxRetries)
        assertEquals(1_000L, policy.initialDelayMs)
        assertEquals(30_000L, policy.maxDelayMs)
        assertEquals(2.0, policy.backoffMultiplier)
    }

    @Test
    fun delayForAttemptExponentialBackoff() {
        val policy = ReconnectPolicy(
            initialDelayMs = 1_000,
            maxDelayMs = 60_000,
            backoffMultiplier = 2.0,
        )
        assertEquals(1_000L, policy.delayForAttempt(0))
        assertEquals(2_000L, policy.delayForAttempt(1))
        assertEquals(4_000L, policy.delayForAttempt(2))
        assertEquals(8_000L, policy.delayForAttempt(3))
        assertEquals(16_000L, policy.delayForAttempt(4))
    }

    @Test
    fun delayClampedToMax() {
        val policy = ReconnectPolicy(
            initialDelayMs = 1_000,
            maxDelayMs = 5_000,
            backoffMultiplier = 3.0,
        )
        assertEquals(1_000L, policy.delayForAttempt(0))
        assertEquals(3_000L, policy.delayForAttempt(1))
        assertEquals(5_000L, policy.delayForAttempt(2)) // clamped: 9_000 -> 5_000
        assertEquals(5_000L, policy.delayForAttempt(3)) // stays clamped
    }

    @Test
    fun noBackoff() {
        val policy = ReconnectPolicy(
            initialDelayMs = 500,
            maxDelayMs = 500,
            backoffMultiplier = 1.0,
        )
        assertEquals(500L, policy.delayForAttempt(0))
        assertEquals(500L, policy.delayForAttempt(1))
        assertEquals(500L, policy.delayForAttempt(5))
    }

    @Test
    fun invalidMaxRetriesThrows() {
        assertFailsWith<IllegalArgumentException> {
            ReconnectPolicy(maxRetries = -1)
        }
    }

    @Test
    fun invalidInitialDelayThrows() {
        assertFailsWith<IllegalArgumentException> {
            ReconnectPolicy(initialDelayMs = 0)
        }
    }

    @Test
    fun maxDelayLessThanInitialThrows() {
        assertFailsWith<IllegalArgumentException> {
            ReconnectPolicy(initialDelayMs = 5_000, maxDelayMs = 1_000)
        }
    }

    @Test
    fun backoffMultiplierLessThanOneThrows() {
        assertFailsWith<IllegalArgumentException> {
            ReconnectPolicy(backoffMultiplier = 0.5)
        }
    }
}
