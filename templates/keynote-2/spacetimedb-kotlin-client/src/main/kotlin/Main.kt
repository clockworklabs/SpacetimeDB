import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.CompressionMode
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.DbConnection
import com.clockworklabs.spacetimedb_kotlin_sdk.shared_client.Status
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.websocket.WebSockets
import jdk.jfr.Configuration
import jdk.jfr.Recording
import kotlinx.coroutines.*
import module_bindings.reducers
import module_bindings.withModuleBindings
import java.io.File
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicLong
import kotlin.random.Random
import kotlin.time.TimeSource

const val DEFAULT_SERVER = "http://localhost:3000"
const val DEFAULT_MODULE = "sim"
const val DEFAULT_DURATION = "5s"
const val DEFAULT_ALPHA = 1.5f
const val DEFAULT_CONNECTIONS = 10
const val DEFAULT_INIT_BALANCE = 1_000_000L
const val DEFAULT_AMOUNT = 1L
const val DEFAULT_ACCOUNTS = 100_000u
const val DEFAULT_MAX_INFLIGHT = 16_384L

fun createHttpClient(): HttpClient = HttpClient(OkHttp) { install(WebSockets) }

suspend fun connect(
    server: String,
    module: String,
    light: Boolean = true,
    confirmed: Boolean = true,
): DbConnection {
    val connected = CompletableDeferred<Unit>()
    val conn = DbConnection.Builder()
        .withHttpClient(createHttpClient())
        .withUri(server)
        .withDatabaseName(module)
        .withLightMode(light)
        .withConfirmedReads(confirmed)
        .withCompression(CompressionMode.NONE)
        .withModuleBindings()
        .onConnect { _, _, _ -> connected.complete(Unit) }
        .onConnectError { _, e -> connected.completeExceptionally(e) }
        .build()
    withTimeout(10_000) { connected.await() }
    return conn
}

fun parseDuration(s: String): Long {
    val trimmed = s.trim()
    return when {
        trimmed.endsWith("ms") -> trimmed.dropLast(2).toLong()
        trimmed.endsWith("s") -> trimmed.dropLast(1).toLong() * 1000
        trimmed.endsWith("m") -> trimmed.dropLast(1).toLong() * 60_000
        else -> trimmed.toLong() * 1000
    }
}

fun pickTwoDistinct(pick: () -> Int, maxSpins: Int = 32): Pair<Int, Int> {
    val a = pick()
    var b = pick()
    var spins = 0
    while (a == b && spins < maxSpins) {
        b = pick()
        spins++
    }
    return a to b
}

fun makeTransfers(accounts: UInt, alpha: Float): List<Pair<UInt, UInt>> {
    val dist = Zipf(accounts.toDouble(), alpha.toDouble(), Random(0x12345678))
    return (0 until 10_000_000).mapNotNull {
        val (from, to) = pickTwoDistinct({ dist.sample() })
        if (from.toUInt() >= accounts || to.toUInt() >= accounts || from == to) null
        else from.toUInt() to to.toUInt()
    }
}

suspend fun seed(
    server: String,
    module: String,
    accounts: UInt,
    initialBalance: Long,
    quiet: Boolean,
) {
    val conn = connect(server, module)
    val done = CompletableDeferred<Unit>()
    conn.reducers.seed(accounts, initialBalance) { ctx ->
        when (val s = ctx.status) {
            Status.Committed -> done.complete(Unit)
            is Status.Failed -> done.completeExceptionally(RuntimeException("seed failed: ${s.message}"))
        }
    }
    withTimeout(60_000) { done.await() }
    if (!quiet) println("done seeding")
    conn.disconnect()
}

suspend fun bench(
    server: String,
    module: String,
    accounts: UInt,
    connections: Int,
    durationMs: Long,
    alpha: Float,
    amount: Long,
    maxInflight: Long,
    quiet: Boolean,
    tpsWritePath: String?,
    confirmed: Boolean,
) {
    if (!quiet) {
        println("Benchmark parameters:")
        println("alpha=$alpha, amount=$amount, accounts=$accounts")
        println("max inflight reducers = $maxInflight")
        println()
        println("initializing $connections connections with confirmed-reads=$confirmed")
    }

    // Open all connections
    val conns = (0 until connections).map { connect(server, module, confirmed = confirmed) }

    // Pre-compute transfer pairs (before any profiling)
    val transferPairs = makeTransfers(accounts, alpha)
    val transfersPerWorker = transferPairs.size / connections
    System.gc() // flush Zipf garbage before profiling
    Thread.sleep(500)
    if (!quiet) System.err.println("benchmarking for ${durationMs}ms...")

    // Start JFR recording for the benchmark window only (not Zipf precompute)
    val jfrFile = System.getenv("JFR_OUTPUT")
    val recording = if (jfrFile != null) {
        Recording(Configuration.getConfiguration("profile")).also {
            it.destination = Path.of(jfrFile)
            it.start()
            if (!quiet) println("JFR recording started -> $jfrFile")
        }
    } else null

    val totalCompleted = AtomicLong(0)
    val clock = TimeSource.Monotonic
    val startMark = clock.markNow()

    coroutineScope {
        conns.forEachIndexed { workerIdx, conn ->
            launch(Dispatchers.Default) {
                val workerStart = clock.markNow()
                var transferIdx = workerIdx * transfersPerWorker

                while (workerStart.elapsedNow().inWholeMilliseconds < durationMs) {
                    // Fire a batch of maxInflight reducers
                    val batchCompleted = CompletableDeferred<Long>()
                    val batchSent = minOf(maxInflight, (transferPairs.size - transferIdx).toLong().coerceAtLeast(0))
                    if (batchSent <= 0) {
                        transferIdx = workerIdx * transfersPerWorker
                        continue
                    }
                    val remaining = AtomicLong(batchSent)

                    for (i in 0 until batchSent.toInt()) {
                        val idx = transferIdx % transferPairs.size
                        transferIdx++
                        val (from, to) = transferPairs[idx]
                        conn.reducers.transfer(from, to, amount) {
                            if (remaining.decrementAndGet() == 0L) {
                                batchCompleted.complete(batchSent)
                            }
                        }
                    }

                    val completed = batchCompleted.await()
                    totalCompleted.addAndGet(completed)
                }
            }
        }
    }

    val elapsed = startMark.elapsedNow().inWholeNanoseconds / 1_000_000_000.0
    val completed = totalCompleted.get()
    val tps = completed / elapsed

    if (!quiet) {
        println("ran for $elapsed seconds")
        println("completed $completed")
    }
    println("throughput was $tps TPS")

    recording?.stop()
    recording?.close()
    if (jfrFile != null && !quiet) println("JFR recording saved -> $jfrFile")

    tpsWritePath?.let { File(it).writeText("$tps") }

    conns.forEach { it.disconnect() }
}

suspend fun main(args: Array<String>) {
    if (args.isEmpty()) {
        println("Usage: <seed|bench> [options]")
        println("  seed  --server URL --module NAME --accounts N --initial-balance N")
        println("  bench --server URL --module NAME --accounts N --connections N --duration Ns --alpha F --amount N --max-inflight N --tps-write-path FILE")
        return
    }

    val cmd = args[0]
    val rest = args.drop(1).toMutableList()
    val quiet = rest.remove("--quiet") || rest.remove("-q")
    val opts = rest.chunked(2).filter { it.size == 2 }.associate { it[0] to it[1] }

    val server = opts["--server"] ?: DEFAULT_SERVER
    val module = opts["--module"] ?: DEFAULT_MODULE
    val accounts = opts["--accounts"]?.toUInt() ?: DEFAULT_ACCOUNTS

    when (cmd) {
        "seed" -> {
            val initialBalance = opts["--initial-balance"]?.toLong() ?: DEFAULT_INIT_BALANCE
            seed(server, module, accounts, initialBalance, quiet)
        }
        "bench" -> {
            val connections = opts["--connections"]?.toInt() ?: DEFAULT_CONNECTIONS
            val durationMs = parseDuration(opts["--duration"] ?: DEFAULT_DURATION)
            val alpha = opts["--alpha"]?.toFloat() ?: DEFAULT_ALPHA
            val amount = opts["--amount"]?.toLong() ?: DEFAULT_AMOUNT
            val maxInflight = opts["--max-inflight"]?.toLong() ?: DEFAULT_MAX_INFLIGHT
            val tpsWritePath = opts["--tps-write-path"]
            val confirmed = opts["--confirmed-reads"]?.toBooleanStrictOrNull() ?: true
            bench(server, module, accounts, connections, durationMs, alpha, amount, maxInflight, quiet, tpsWritePath, confirmed)
        }
        else -> {
            System.err.println("Unknown command: $cmd (expected 'seed' or 'bench')")
        }
    }
}
