import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.protocol.QueryResult
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertTrue

class OneOffQueryTest {

    @Test
    fun `callback oneOffQuery with valid SQL returns Ok result`() = runBlocking {
        val client = connectToDb()

        val result = CompletableDeferred<QueryResult>()
        client.conn.oneOffQuery("SELECT * FROM user") { msg ->
            result.complete(msg.result)
        }

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) { result.await() }
        assertTrue(qr is QueryResult.Ok, "Valid SQL should return QueryResult.Ok, got: $qr")

        client.conn.disconnect()
    }

    @Test
    fun `callback oneOffQuery with invalid SQL returns Err result`() = runBlocking {
        val client = connectToDb()

        val result = CompletableDeferred<QueryResult>()
        client.conn.oneOffQuery("THIS IS NOT VALID SQL AT ALL") { msg ->
            result.complete(msg.result)
        }

        val qr = withTimeout(DEFAULT_TIMEOUT_MS) { result.await() }
        assertTrue(qr is QueryResult.Err, "Invalid SQL should return QueryResult.Err, got: $qr")
        assertTrue((qr as QueryResult.Err).error.isNotEmpty(), "Error message should be non-empty")

        client.conn.disconnect()
    }

    @Test
    fun `suspend oneOffQuery with valid SQL returns Ok result`() = runBlocking {
        val client = connectToDb()

        val msg = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("SELECT * FROM user")
        }
        assertTrue(msg.result is QueryResult.Ok, "Valid SQL should return QueryResult.Ok, got: ${msg.result}")

        client.conn.disconnect()
    }

    @Test
    fun `suspend oneOffQuery with invalid SQL returns Err result`() = runBlocking {
        val client = connectToDb()

        val msg = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("INVALID SQL QUERY")
        }
        assertTrue(msg.result is QueryResult.Err, "Invalid SQL should return QueryResult.Err, got: ${msg.result}")

        client.conn.disconnect()
    }

    @Test
    fun `oneOffQuery returns rows with table data for populated table`() = runBlocking {
        val client = connectToDb()

        val msg = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("SELECT * FROM user")
        }
        val qr = msg.result
        assertTrue(qr is QueryResult.Ok, "Should return Ok")
        qr as QueryResult.Ok
        // We are connected, so at least our own user row should exist
        assertTrue(qr.rows.tables.isNotEmpty(), "Should have at least 1 table in result")
        assertTrue(qr.rows.tables[0].rows.rowsSize > 0, "Should have row data bytes for populated table")

        client.conn.disconnect()
    }

    @Test
    fun `oneOffQuery returns Ok with empty rows for nonexistent filter`() = runBlocking {
        val client = connectToDb()

        val msg = withTimeout(DEFAULT_TIMEOUT_MS) {
            client.conn.oneOffQuery("SELECT * FROM note WHERE tag = 'nonexistent-tag-xyz-12345'")
        }
        val qr = msg.result
        assertTrue(qr is QueryResult.Ok, "Valid SQL should return Ok even with 0 rows")

        client.conn.disconnect()
    }

    @Test
    fun `multiple concurrent oneOffQueries all return`() = runBlocking {
        val client = connectToDb()

        val results = (1..5).map { i ->
            val deferred = CompletableDeferred<QueryResult>()
            client.conn.oneOffQuery("SELECT * FROM user") { msg ->
                deferred.complete(msg.result)
            }
            deferred
        }

        results.forEachIndexed { i, deferred ->
            val qr = withTimeout(DEFAULT_TIMEOUT_MS) { deferred.await() }
            assertTrue(qr is QueryResult.Ok, "Query $i should return Ok, got: $qr")
        }

        client.conn.disconnect()
    }
}
