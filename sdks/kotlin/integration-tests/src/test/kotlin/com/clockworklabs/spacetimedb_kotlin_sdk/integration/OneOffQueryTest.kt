package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.OneOffQueryData
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.OneOffQueryResult
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.QueryError
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.SdkResult
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.getOrNull
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertIs
import kotlin.test.assertTrue

class OneOffQueryTest {

    @Test
    fun `callback oneOffQuery with valid SQL returns Success`() = runBlocking {
        val client = connectToDb()

        val result = CompletableDeferred<OneOffQueryResult>()
        client.conn.oneOffQuery("SELECT * FROM user") { msg ->
            result.complete(msg)
        }

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) { result.await() }
        assertIs<SdkResult.Success<*>>(qr, "Valid SQL should return Success, got: $qr")

        client.conn.disconnect()
    }

    @Test
    fun `callback oneOffQuery with invalid SQL returns Failure`() = runBlocking {
        val client = connectToDb()

        val result = CompletableDeferred<OneOffQueryResult>()
        client.conn.oneOffQuery("THIS IS NOT VALID SQL AT ALL") { msg ->
            result.complete(msg)
        }

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) { result.await() }
        assertIs<SdkResult.Failure<*>>(qr, "Invalid SQL should return Failure, got: $qr")
        val serverError = assertIs<QueryError.ServerError>(qr.error, "Error should be QueryError.ServerError")
        assertTrue(serverError.message.isNotEmpty(), "Error message should be non-empty")

        client.conn.disconnect()
    }

    @Test
    fun `suspend oneOffQuery with valid SQL returns Success`() = runBlocking {
        val client = connectToDb()

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("SELECT * FROM user")
        }
        assertIs<SdkResult.Success<OneOffQueryData>>(qr, "Valid SQL should return Success, got: $qr")
        assertTrue(qr.getOrNull()!!.tableCount >= 0, "tableCount should be non-negative")

        client.conn.disconnect()
    }

    @Test
    fun `suspend oneOffQuery with invalid SQL returns Failure`() = runBlocking {
        val client = connectToDb()

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("INVALID SQL QUERY")
        }
        assertIs<SdkResult.Failure<*>>(qr, "Invalid SQL should return Failure, got: $qr")

        client.conn.disconnect()
    }

    @Test
    fun `oneOffQuery returns Success with tableCount for populated table`() = runBlocking {
        val client = connectToDb()

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("SELECT * FROM user")
        }
        assertIs<SdkResult.Success<OneOffQueryData>>(qr, "Should return Success")
        assertTrue(qr.getOrNull()!!.tableCount > 0, "Should have at least 1 table in result")

        client.conn.disconnect()
    }

    @Test
    fun `oneOffQuery returns Success for nonexistent filter`() = runBlocking {
        val client = connectToDb()

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("SELECT * FROM note WHERE tag = 'nonexistent-tag-xyz-12345'")
        }
        assertIs<SdkResult.Success<*>>(qr, "Valid SQL should return Success even with 0 rows")

        client.conn.disconnect()
    }

    @Test
    fun `multiple concurrent oneOffQueries all return`() = runBlocking {
        val client = connectToDb()

        val results = (1..5).map { _ ->
            val deferred = CompletableDeferred<OneOffQueryResult>()
            client.conn.oneOffQuery("SELECT * FROM user") { msg ->
                deferred.complete(msg)
            }
            deferred
        }

        results.forEachIndexed { i, deferred ->
            val qr = withTimeout(DEFAULT_TIMEOUT_MS) { deferred.await() }
            assertIs<SdkResult.Success<*>>(qr, "Query $i should return Success, got: $qr")
        }

        client.conn.disconnect()
    }
}
