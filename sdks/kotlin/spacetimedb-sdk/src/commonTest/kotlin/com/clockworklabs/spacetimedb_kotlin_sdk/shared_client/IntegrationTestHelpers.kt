package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.bsatn.BsatnWriter
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.*
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.ConnectionId
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.type.Identity
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.transport.Transport
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope

val TEST_IDENTITY = Identity(BigInteger.ONE)
val TEST_CONNECTION_ID = ConnectionId(BigInteger.TWO)
const val TEST_TOKEN = "test-token-abc"

fun initialConnectionMsg() = ServerMessage.InitialConnection(
    identity = TEST_IDENTITY,
    connectionId = TEST_CONNECTION_ID,
    token = TEST_TOKEN,
)

internal suspend fun TestScope.buildTestConnection(
    transport: FakeTransport,
    onConnect: ((DbConnectionView, Identity, String) -> Unit)? = null,
    onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
    onConnectError: ((DbConnectionView, Throwable) -> Unit)? = null,
    moduleDescriptor: ModuleDescriptor? = null,
    callbackDispatcher: kotlinx.coroutines.CoroutineDispatcher? = null,
    exceptionHandler: CoroutineExceptionHandler? = null,
): DbConnection {
    val conn = createTestConnection(transport, onConnect, onDisconnect, onConnectError, moduleDescriptor, callbackDispatcher, exceptionHandler)
    conn.connect()
    return conn
}

internal fun TestScope.createTestConnection(
    transport: FakeTransport,
    onConnect: ((DbConnectionView, Identity, String) -> Unit)? = null,
    onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
    onConnectError: ((DbConnectionView, Throwable) -> Unit)? = null,
    moduleDescriptor: ModuleDescriptor? = null,
    callbackDispatcher: kotlinx.coroutines.CoroutineDispatcher? = null,
    exceptionHandler: CoroutineExceptionHandler? = null,
): DbConnection {
    val baseContext = SupervisorJob() + StandardTestDispatcher(testScheduler)
    val context = if (exceptionHandler != null) baseContext + exceptionHandler else baseContext
    return DbConnection(
        transport = transport,
        scope = CoroutineScope(context),
        onConnectCallbacks = listOfNotNull(onConnect),
        onDisconnectCallbacks = listOfNotNull(onDisconnect),
        onConnectErrorCallbacks = listOfNotNull(onConnectError),
        clientConnectionId = ConnectionId.random(),
        stats = Stats(),
        moduleDescriptor = moduleDescriptor,
        callbackDispatcher = callbackDispatcher,
    )
}

internal fun TestScope.createConnectionWithTransport(
    transport: Transport,
    onDisconnect: ((DbConnectionView, Throwable?) -> Unit)? = null,
): DbConnection {
    return DbConnection(
        transport = transport,
        scope = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)),
        onConnectCallbacks = emptyList(),
        onDisconnectCallbacks = listOfNotNull(onDisconnect),
        onConnectErrorCallbacks = emptyList(),
        clientConnectionId = ConnectionId.random(),
        stats = Stats(),
        moduleDescriptor = null,
        callbackDispatcher = null,
    )
}

fun emptyQueryRows(): QueryRows = QueryRows(emptyList())

fun transactionUpdateMsg(
    querySetId: QuerySetId,
    tableName: String,
    inserts: BsatnRowList = buildRowList(),
    deletes: BsatnRowList = buildRowList(),
) = ServerMessage.TransactionUpdateMsg(
    TransactionUpdate(
        listOf(
            QuerySetUpdate(
                querySetId,
                listOf(
                    TableUpdate(
                        tableName,
                        listOf(TableUpdateRows.PersistentTable(inserts, deletes))
                    )
                )
            )
        )
    )
)

fun encodeInitialConnectionBytes(): ByteArray {
    val writer = BsatnWriter()
    writer.writeSumTag(0u) // InitialConnection tag
    TEST_IDENTITY.encode(writer)
    TEST_CONNECTION_ID.encode(writer)
    writer.writeString(TEST_TOKEN)
    return writer.toByteArray()
}
