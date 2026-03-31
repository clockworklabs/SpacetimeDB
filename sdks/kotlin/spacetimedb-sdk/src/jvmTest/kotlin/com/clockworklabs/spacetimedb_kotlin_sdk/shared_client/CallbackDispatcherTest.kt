package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.ServerMessage
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.DelicateCoroutinesApi
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.newSingleThreadContext
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class, DelicateCoroutinesApi::class)
class CallbackDispatcherTest {

    private val testIdentity = Identity(BigInteger.ONE)
    private val testConnectionId = ConnectionId(BigInteger.TWO)
    private val testToken = "test-token-abc"

    private fun initialConnectionMsg() = ServerMessage.InitialConnection(
        identity = testIdentity,
        connectionId = testConnectionId,
        token = testToken,
    )

    @Test
    fun `callback dispatcher is used for callbacks`() = runTest {
        val transport = FakeTransport()

        val callbackDispatcher = newSingleThreadContext("TestCallbackThread")
        val callbackThreadDeferred = CompletableDeferred<String>()

        callbackDispatcher.use { callbackDispatcher ->
            val conn = DbConnection(
                transport = transport,
                scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
                onConnectCallbacks = listOf { _, _, _ ->
                    callbackThreadDeferred.complete(Thread.currentThread().name)
                },
                onDisconnectCallbacks = emptyList(),
                onConnectErrorCallbacks = emptyList(),
                clientConnectionId = ConnectionId.random(),
                stats = Stats(),
                moduleDescriptor = null,
                callbackDispatcher = callbackDispatcher,
            )
            conn.connect()
            transport.sendToClient(initialConnectionMsg())
            advanceUntilIdle()

            val capturedThread = callbackThreadDeferred.await()
            advanceUntilIdle()
            assertNotNull(capturedThread)
            assertTrue(capturedThread.contains("TestCallbackThread"))
            conn.disconnect()
        }
    }
}
