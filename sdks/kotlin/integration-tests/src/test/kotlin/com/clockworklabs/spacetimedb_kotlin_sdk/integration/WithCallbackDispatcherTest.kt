package com.clockworklabs.spacetimedb_kotlin_sdk.integration

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import module_bindings.reducers
import module_bindings.withModuleBindings
import java.util.concurrent.Executors
import kotlin.test.Test
import kotlin.test.assertTrue

class WithCallbackDispatcherTest {

    private fun createNamedDispatcher(name: String): Pair<kotlinx.coroutines.ExecutorCoroutineDispatcher, java.util.concurrent.ExecutorService> {
        val executor = Executors.newSingleThreadExecutor { r -> Thread(r, name) }
        return executor.asCoroutineDispatcher() to executor
    }

    @Test
    fun `onConnect callback runs on custom dispatcher`() = runBlocking {
        val (dispatcher, executor) = createNamedDispatcher("custom-cb-thread")

        val threadName = CompletableDeferred<String>()
        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .withCallbackDispatcher(dispatcher)
            .onConnect { _, _, _ -> threadName.complete(Thread.currentThread().name) }
            .onConnectError { _, e -> threadName.completeExceptionally(e) }
            .build()

        val name = withTimeout(DEFAULT_TIMEOUT_MS) { threadName.await() }
        assertTrue(name.startsWith("custom-cb-thread"), "onConnect should run on custom thread, got: $name")

        conn.disconnect()
        dispatcher.close()
        executor.shutdown()
    }

    @Test
    fun `subscription onApplied callback runs on custom dispatcher`() = runBlocking {
        val (dispatcher, executor) = createNamedDispatcher("sub-cb-thread")

        val connected = CompletableDeferred<Unit>()
        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .withCallbackDispatcher(dispatcher)
            .onConnect { _, _, _ -> connected.complete(Unit) }
            .onConnectError { _, e -> connected.completeExceptionally(e) }
            .build()

        withTimeout(DEFAULT_TIMEOUT_MS) { connected.await() }

        val threadName = CompletableDeferred<String>()
        conn.subscriptionBuilder()
            .onApplied { _ -> threadName.complete(Thread.currentThread().name) }
            .onError { _, err -> threadName.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")

        val name = withTimeout(DEFAULT_TIMEOUT_MS) { threadName.await() }
        assertTrue(name.startsWith("sub-cb-thread"), "onApplied should run on custom thread, got: $name")

        conn.disconnect()
        dispatcher.close()
        executor.shutdown()
    }

    @Test
    fun `reducer callback runs on custom dispatcher`() = runBlocking {
        val (dispatcher, executor) = createNamedDispatcher("reducer-cb-thread")

        val connected = CompletableDeferred<Unit>()
        val conn = DbConnection.Builder()
            .withHttpClient(createTestHttpClient())
            .withUri(HOST)
            .withDatabaseName(DB_NAME)
            .withModuleBindings()
            .withCallbackDispatcher(dispatcher)
            .onConnect { _, _, _ ->
                connected.complete(Unit)
            }
            .onConnectError { _, e -> connected.completeExceptionally(e) }
            .build()

        withTimeout(DEFAULT_TIMEOUT_MS) { connected.await() }

        // Subscribe first so reducer callbacks can fire
        val subApplied = CompletableDeferred<Unit>()
        conn.subscriptionBuilder()
            .onApplied { _ -> subApplied.complete(Unit) }
            .onError { _, err -> subApplied.completeExceptionally(RuntimeException("$err")) }
            .subscribe("SELECT * FROM user")
        withTimeout(DEFAULT_TIMEOUT_MS) { subApplied.await() }

        val threadName = CompletableDeferred<String>()
        conn.reducers.onSetName { _, _ ->
            threadName.complete(Thread.currentThread().name)
        }
        conn.reducers.setName("dispatcher-test-${System.nanoTime()}")

        val name = withTimeout(DEFAULT_TIMEOUT_MS) { threadName.await() }
        assertTrue(name.startsWith("reducer-cb-thread"), "reducer callback should run on custom thread, got: $name")

        conn.disconnect()
        dispatcher.close()
        executor.shutdown()
    }
}
