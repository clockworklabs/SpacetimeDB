package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.withModuleBindings
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class DbConnectionBuilderErrorTest {

    @Test
    fun `build with invalid URI fires onConnectError`() = runBlocking {
        val error = CompletableDeferred<Throwable>()

        DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri("ws://localhost:99999")
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .onConnect { _, _, _ -> error.completeExceptionally(AssertionError("Should not connect")) }
            .onConnectError { _, e -> error.complete(e) }
            .build()

        val ex = withTimeout(DEFAULT_TIMEOUT_MS) { error.await() }
        assertNotNull(ex, "Should receive an error on invalid URI")
        Unit
    }

    @Test
    fun `build with unreachable host fires onConnectError`() = runBlocking {
        val error = CompletableDeferred<Throwable>()

        DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri("ws://192.0.2.1:3000")
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .onConnect { _, _, _ -> error.completeExceptionally(AssertionError("Should not connect")) }
            .onConnectError { _, e -> error.complete(e) }
            .build()

        val ex = withTimeout(DEFAULT_TIMEOUT_MS) { error.await() }
        assertNotNull(ex, "Should receive an error on unreachable host")
        Unit
    }

    @Test
    fun `build with invalid database name fires onConnectError`() = runBlocking {
        val error = CompletableDeferred<Throwable>()

        DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName("nonexistent-db-${System.nanoTime()}")
            .withModuleBindings()
            .onConnect { _, _, _ -> error.completeExceptionally(AssertionError("Should not connect")) }
            .onConnectError { _, e -> error.complete(e) }
            .build()

        val ex = withTimeout(DEFAULT_TIMEOUT_MS) { error.await() }
        assertNotNull(ex, "Should receive an error on invalid database name")
        Unit
    }

    @Test
    fun `isActive is false after connect error`() = runBlocking {
        val error = CompletableDeferred<Throwable>()

        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri("ws://localhost:99999")
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .onConnectError { _, e -> error.complete(e) }
            .build()

        withTimeout(DEFAULT_TIMEOUT_MS) { error.await() }
        assertTrue(!conn.isActive, "isActive should be false after connect error")
    }

    @Test
    fun `build with garbage token fires onConnectError`() = runBlocking {
        val error = CompletableDeferred<Throwable>()

        DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withToken("not-a-valid-token")
            .withModuleBindings()
            .onConnect { _, _, _ -> error.completeExceptionally(AssertionError("Should not connect with invalid token")) }
            .onConnectError { _, e -> error.complete(e) }
            .build()

        val ex = withTimeout(DEFAULT_TIMEOUT_MS) { error.await() }
        assertNotNull(ex, "Should receive an error on invalid token")
        assertEquals(ex.message?.contains("401"), true, "Error should mention 401: ${ex.message}")
    }
}
