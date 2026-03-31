package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals

class TokenReconnectTest {

    @Test
    fun `reconnect with saved token returns same identity`() = runBlocking {
        val first = connectToDb()
        val savedToken = first.token
        val savedIdentity = first.identity
        first.conn.disconnect()

        val second = connectToDb(token = savedToken)
        assertEquals(savedIdentity, second.identity, "Identity should be the same when reconnecting with saved token")
        second.conn.disconnect()
    }

    @Test
    fun `reconnect with saved token returns same token`() = runBlocking {
        val first = connectToDb()
        val savedToken = first.token
        first.conn.disconnect()

        val second = connectToDb(token = savedToken)
        assertEquals(savedToken, second.token, "Token should be the same when reconnecting")
        second.conn.disconnect()
    }

    @Test
    fun `connect without token generates new identity each time`() = runBlocking {
        val first = connectToDb()
        val firstIdentity = first.identity
        first.conn.disconnect()

        val second = connectToDb()
        assertNotEquals(firstIdentity, second.identity, "Different anonymous connections should have different identities")
        second.conn.disconnect()
    }

    @Test
    fun `connect without token generates new token each time`() = runBlocking {
        val first = connectToDb()
        val firstToken = first.token
        first.conn.disconnect()

        val second = connectToDb()
        assertNotEquals(firstToken, second.token, "Different anonymous connections should have different tokens")
        second.conn.disconnect()
    }

    @Test
    fun `token from first connection works after multiple reconnects`() = runBlocking {
        val first = connectToDb()
        val savedToken = first.token
        val savedIdentity = first.identity
        first.conn.disconnect()

        // Reconnect 3 times with same token
        for (i in 1..3) {
            val client = connectToDb(token = savedToken)
            assertEquals(savedIdentity, client.identity, "Identity should match on reconnect #$i")
            client.conn.disconnect()
        }
    }
}
