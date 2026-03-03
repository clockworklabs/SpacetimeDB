import Foundation
import Atomics
import SpacetimeDB

enum CliError: Error, CustomStringConvertible {
    case invalidCommand(String)
    case missingValue(String)
    case invalidValue(String, String)
    case invalidDuration(String)
    case invalidURL(String)
    case timeout(String)

    var description: String {
        switch self {
        case .invalidCommand(let cmd):
            return "invalid command: \(cmd)"
        case .missingValue(let arg):
            return "missing value for argument: \(arg)"
        case .invalidValue(let arg, let value):
            return "invalid value for \(arg): \(value)"
        case .invalidDuration(let value):
            return "invalid duration: \(value)"
        case .invalidURL(let value):
            return "invalid server URL: \(value)"
        case .timeout(let message):
            return message
        }
    }
}

struct CommonOptions {
    var quiet = false
    var server = "http://localhost:3000"
    var module = "sim"
    var accounts: UInt32 = 100_000
    var amount: UInt32 = 1
}

struct SeedOptions {
    var initialBalance: Int64 = 1_000_000
    var waitTimeoutSeconds: Double = 60
}

struct BenchOptions {
    var durationSeconds: Double = 5
    var alpha: Double = 1.5
    var connections: Int = 10
    var maxInflightReducers: UInt64 = 16_384
    var tpsWritePath: String?
}

enum Command {
    case seed(CommonOptions, SeedOptions)
    case bench(CommonOptions, BenchOptions)
}

struct SeedArgs: Encodable {
    let n: UInt32
    let balance: Int64
}

struct TransferArgs: Encodable {
    let from: UInt32
    let to: UInt32
    let amount: UInt32
}

final class CompletionCounter: @unchecked Sendable {
    private let applied = ManagedAtomic<UInt64>(0)
    private let errors = ManagedAtomic<UInt64>(0)

    func recordApplied() {
        applied.wrappingIncrement(ordering: .relaxed)
    }

    func recordApplied(count: UInt64) {
        applied.wrappingIncrement(by: count, ordering: .relaxed)
    }

    func recordError() {
        errors.wrappingIncrement(ordering: .relaxed)
    }

    func snapshot() -> (applied: UInt64, errors: UInt64) {
        (
            applied.load(ordering: .relaxed),
            errors.load(ordering: .relaxed)
        )
    }
}

@MainActor
final class BenchDelegate: SpacetimeClientDelegate {
    let counter: CompletionCounter

    init(counter: CompletionCounter) {
        self.counter = counter
    }

    func onConnect() {}
    func onDisconnect(error _: Error?) {}
    func onConnectError(error _: Error) {}
    func onConnectionStateChange(state _: ConnectionState) {}
    func onIdentityReceived(identity _: [UInt8], token _: String) {}

    func onTransactionUpdate(message _: Data?) {
        // Completion accounting uses raw transaction observer callback.
    }

    func onReducerError(reducer _: String, message _: String, isInternal _: Bool) {
        counter.recordError()
    }
}

struct SplitMix64: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) {
        state = seed
    }

    mutating func next() -> UInt64 {
        state &+= 0x9E3779B97F4A7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58476D1CE4E5B9
        z = (z ^ (z >> 27)) &* 0x94D049BB133111EB
        return z ^ (z >> 31)
    }

    mutating func nextUnitInterval() -> Double {
        Double(next()) / Double(UInt64.max)
    }
}

struct ZipfSampler {
    private let cdf: [Double]

    init(accountCount: UInt32, alpha: Double) {
        let n = Int(accountCount)
        var weights = Array(repeating: 0.0, count: n)
        var sum = 0.0
        for i in 0..<n {
            let rank = Double(i + 1)
            let w = 1.0 / pow(rank, alpha)
            weights[i] = w
            sum += w
        }

        var running = 0.0
        var nextCDF = Array(repeating: 0.0, count: n)
        for i in 0..<n {
            running += weights[i] / sum
            nextCDF[i] = running
        }
        cdf = nextCDF
    }

    func sample(using rng: inout SplitMix64) -> UInt32 {
        let needle = rng.nextUnitInterval()
        var lo = 0
        var hi = cdf.count - 1

        while lo < hi {
            let mid = (lo + hi) / 2
            if needle <= cdf[mid] {
                hi = mid
            } else {
                lo = mid + 1
            }
        }

        return UInt32(lo + 1) // 1-based, matching Rust-client sampling behavior.
    }
}

func makeTransfers(accounts: UInt32, alpha: Double, count: Int = 10_000_000) -> [(UInt32, UInt32)] {
    let sampler = ZipfSampler(accountCount: accounts, alpha: alpha)
    var rng = SplitMix64(seed: 0x1234_5678)
    var transfers: [(UInt32, UInt32)] = []
    transfers.reserveCapacity(count)

    while transfers.count < count {
        let from = sampler.sample(using: &rng)
        let to = sampler.sample(using: &rng)
        if from >= accounts || to >= accounts || from == to {
            continue
        }
        transfers.append((from, to))
    }

    return transfers
}

func parseDurationSeconds(_ raw: String) throws -> Double {
    let lowered = raw.lowercased()
    if lowered.hasSuffix("ms") {
        let value = String(lowered.dropLast(2))
        guard let ms = Double(value), ms > 0 else { throw CliError.invalidDuration(raw) }
        return ms / 1000
    }
    if lowered.hasSuffix("s") {
        let value = String(lowered.dropLast())
        guard let seconds = Double(value), seconds > 0 else { throw CliError.invalidDuration(raw) }
        return seconds
    }
    if lowered.hasSuffix("m") {
        let value = String(lowered.dropLast())
        guard let minutes = Double(value), minutes > 0 else { throw CliError.invalidDuration(raw) }
        return minutes * 60
    }
    if lowered.hasSuffix("h") {
        let value = String(lowered.dropLast())
        guard let hours = Double(value), hours > 0 else { throw CliError.invalidDuration(raw) }
        return hours * 3600
    }
    throw CliError.invalidDuration(raw)
}

func parseUInt32(_ value: String, argName: String) throws -> UInt32 {
    guard let parsed = UInt32(value) else {
        throw CliError.invalidValue(argName, value)
    }
    return parsed
}

func parseInt64(_ value: String, argName: String) throws -> Int64 {
    guard let parsed = Int64(value) else {
        throw CliError.invalidValue(argName, value)
    }
    return parsed
}

func parseDouble(_ value: String, argName: String) throws -> Double {
    guard let parsed = Double(value) else {
        throw CliError.invalidValue(argName, value)
    }
    return parsed
}

func parseInt(_ value: String, argName: String) throws -> Int {
    guard let parsed = Int(value) else {
        throw CliError.invalidValue(argName, value)
    }
    return parsed
}

func parseArgs() throws -> Command {
    var argv = Array(CommandLine.arguments.dropFirst())
    guard let commandRaw = argv.first else {
        throw CliError.invalidCommand("<none>")
    }
    argv.removeFirst()

    var values: [String: String] = [:]
    var flags = Set<String>()

    var i = 0
    while i < argv.count {
        let token = argv[i]
        if token == "-q" || token == "--quiet" {
            flags.insert("quiet")
            i += 1
            continue
        }

        guard token.hasPrefix("--") else {
            throw CliError.invalidValue("argument", token)
        }

        if let eq = token.firstIndex(of: "=") {
            let key = String(token[token.startIndex..<eq])
            let valueStart = token.index(after: eq)
            let value = String(token[valueStart...])
            values[key] = value
            i += 1
            continue
        }

        guard i + 1 < argv.count else { throw CliError.missingValue(token) }
        values[token] = argv[i + 1]
        i += 2
    }

    var common = CommonOptions()
    common.quiet = flags.contains("quiet")
    if let server = values["--server"] { common.server = server }
    if let module = values["--module"] { common.module = module }
    if let accounts = values["--accounts"] { common.accounts = try parseUInt32(accounts, argName: "--accounts") }
    if let amount = values["--amount"] { common.amount = try parseUInt32(amount, argName: "--amount") }

    switch commandRaw {
    case "seed":
        var seed = SeedOptions()
        if let initial = values["--initial-balance"] {
            seed.initialBalance = try parseInt64(initial, argName: "--initial-balance")
        }
        if let timeout = values["--wait-timeout-seconds"] {
            seed.waitTimeoutSeconds = try parseDouble(timeout, argName: "--wait-timeout-seconds")
        }
        return .seed(common, seed)

    case "bench":
        var bench = BenchOptions()
        if let duration = values["--duration"] {
            bench.durationSeconds = try parseDurationSeconds(duration)
        }
        if let alpha = values["--alpha"] {
            bench.alpha = try parseDouble(alpha, argName: "--alpha")
        }
        if let connections = values["--connections"] {
            bench.connections = try parseInt(connections, argName: "--connections")
        }
        if let inflight = values["--max-inflight-reducers"] {
            let parsed = try parseInt(inflight, argName: "--max-inflight-reducers")
            guard parsed > 0 else {
                throw CliError.invalidValue("--max-inflight-reducers", inflight)
            }
            bench.maxInflightReducers = UInt64(parsed)
        }
        if let path = values["--tps-write-path"] {
            bench.tpsWritePath = path
        }
        return .bench(common, bench)

    default:
        throw CliError.invalidCommand(commandRaw)
    }
}

@MainActor
func makeClient(common: CommonOptions, counter: CompletionCounter) throws -> (client: SpacetimeClient, delegate: BenchDelegate) {
    guard let url = URL(string: common.server) else {
        throw CliError.invalidURL(common.server)
    }

    let client = SpacetimeClient(
        serverUrl: url,
        moduleName: common.module,
        reconnectPolicy: nil,
        compressionMode: .none
    )
    client.setRawTransactionUpdateCountObserver { appliedCount in
        counter.recordApplied(count: appliedCount)
    }
    let delegate = BenchDelegate(counter: counter)
    client.delegate = delegate
    return (client, delegate)
}

func waitForConnected(_ client: SpacetimeClient, timeoutSeconds: Double) async throws {
    let deadline = Date().addingTimeInterval(timeoutSeconds)
    while Date() < deadline {
        let state = await MainActor.run { client.connectionState }
        if state == .connected {
            return
        }
        try await Task.sleep(nanoseconds: 10_000_000)
    }
    throw CliError.timeout("timed out waiting for connection")
}

func waitForAcks(counter: CompletionCounter, targetTotalAcks: UInt64, timeoutSeconds: Double) async throws {
    let deadline = Date().addingTimeInterval(timeoutSeconds)
    while Date() < deadline {
        let snapshot = counter.snapshot()
        if snapshot.applied + snapshot.errors >= targetTotalAcks {
            return
        }
        try await Task.sleep(nanoseconds: 1_000_000)
    }
    throw CliError.timeout("timed out waiting for reducer acknowledgements")
}

func runSeed(common: CommonOptions, seed: SeedOptions) async throws {
    let counter = CompletionCounter()
    let setup = try await MainActor.run { try makeClient(common: common, counter: counter) }
    let client = setup.client
    _ = setup.delegate

    await MainActor.run { SpacetimeClient.clientCache = ClientCache() }
    await MainActor.run { client.connect() }
    try await waitForConnected(client, timeoutSeconds: 20)

    let encoder = BSATNEncoder()
    let seedArgs = SeedArgs(n: common.accounts, balance: seed.initialBalance)
    let payload = try encoder.encode(seedArgs)

    let before = counter.snapshot()
    client.send("seed", payload)
    try await waitForAcks(
        counter: counter,
        targetTotalAcks: (before.applied + before.errors) + 1,
        timeoutSeconds: seed.waitTimeoutSeconds
    )

    await MainActor.run { client.disconnect() }

    if !common.quiet {
        print("done seeding")
    }
}

func runBench(common: CommonOptions, bench: BenchOptions) async throws {
    let durationSeconds = bench.durationSeconds
    let transfers = makeTransfers(accounts: common.accounts, alpha: bench.alpha)
    let transfersPerWorker = max(1, transfers.count / max(1, bench.connections))

    if !common.quiet {
        print("Benchmark parameters:")
        print("alpha=\(bench.alpha), amount=\(common.amount), accounts=\(common.accounts)")
        print("max inflight reducers = \(bench.maxInflightReducers)")
        print()
        print("benchmarking for \(durationSeconds)s...")
        print("initializing \(bench.connections) connections")
    }

    var clients: [SpacetimeClient] = []
    var delegates: [BenchDelegate] = []
    var counters: [CompletionCounter] = []
    clients.reserveCapacity(bench.connections)
    delegates.reserveCapacity(bench.connections)
    counters.reserveCapacity(bench.connections)

    await MainActor.run { SpacetimeClient.clientCache = ClientCache() }

    for _ in 0..<bench.connections {
        let counter = CompletionCounter()
        let setup = try await MainActor.run { try makeClient(common: common, counter: counter) }
        counters.append(counter)
        clients.append(setup.client)
        delegates.append(setup.delegate)
    }

    for client in clients {
        await MainActor.run { client.connect() }
    }
    for client in clients {
        try await waitForConnected(client, timeoutSeconds: 20)
    }

    let start = Date()
    let stopAt = start.addingTimeInterval(durationSeconds)

    let completed: UInt64 = try await withThrowingTaskGroup(of: UInt64.self) { group in
        for idx in 0..<clients.count {
            let client = clients[idx]
            let counter = counters[idx]
            let startIndex = idx * transfersPerWorker
            let maxInflightReducers = bench.maxInflightReducers
            let amount = common.amount

            group.addTask {
                var localCompleted: UInt64 = 0
                var transferIndex = startIndex
                let encoder = BSATNEncoder()

                while Date() < stopAt {
                    let before = counter.snapshot()
                    var sent: UInt64 = 0

                    while sent < maxInflightReducers {
                        let pair = transfers[transferIndex]
                        transferIndex += 1
                        if transferIndex >= transfers.count {
                            transferIndex = 0
                        }

                        let args = TransferArgs(from: pair.0, to: pair.1, amount: amount)
                        let payload = try encoder.encode(args)
                        client.send("transfer", payload)
                        sent &+= 1
                    }

                    let ackTarget = (before.applied + before.errors) + sent
                    try await waitForAcks(counter: counter, targetTotalAcks: ackTarget, timeoutSeconds: 60)
                    let after = counter.snapshot()
                    localCompleted &+= (after.applied - before.applied)
                }

                return localCompleted
            }
        }

        var total: UInt64 = 0
        for try await value in group {
            total &+= value
        }
        return total
    }

    let elapsed = Date().timeIntervalSince(start)
    let tps = Double(completed) / elapsed

    for client in clients {
        await MainActor.run { client.disconnect() }
    }

    if !common.quiet {
        print("ran for \(elapsed) seconds")
        print("completed \(completed)")
        print("throughput was \(tps) TPS")
    }

    if let tpsWritePath = bench.tpsWritePath {
        try String(tps).write(toFile: tpsWritePath, atomically: true, encoding: .utf8)
    }
}

@main
struct SpacetimeDBSwiftTransferSim {
    static func main() async {
        do {
            switch try parseArgs() {
            case .seed(let common, let seed):
                try await runSeed(common: common, seed: seed)
            case .bench(let common, let bench):
                try await runBench(common: common, bench: bench)
            }
        } catch {
            fputs("error: \(error)\n", stderr)
            exit(1)
        }
    }
}
