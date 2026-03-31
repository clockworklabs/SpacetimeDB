package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class DbConnectionIsActiveTest {

    @Test
    fun `isActive reflects connection lifecycle`() = runBlocking {
        val client = connectToDb()

        assertTrue(client.conn.isActive, "Should be active after connect")

        client.conn.disconnect()

        assertFalse(client.conn.isActive, "Should be inactive after disconnect")
    }
}
