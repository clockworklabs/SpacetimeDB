import Benchmark
import Foundation
import SpacetimeDB

// MARK: - Test Data Structures

struct Point3D: Codable, Sendable, BSATNSpecialDecodable, BSATNSpecialEncodable {
    var x: Float
    var y: Float
    var z: Float

    static func decodeBSATN(from reader: inout BSATNReader) throws -> Point3D {
        return Point3D(
            x: try reader.readFloat(),
            y: try reader.readFloat(),
            z: try reader.readFloat()
        )
    }

    func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.appendFloat(x)
        storage.appendFloat(y)
        storage.appendFloat(z)
    }
}

struct PlayerRow: Codable, Sendable, BSATNSpecialDecodable, BSATNSpecialEncodable {
    var id: UInt64
    var name: String
    var x: Float
    var y: Float
    var health: UInt32
    var weaponCount: UInt32
    var kills: UInt32
    var respawnAtMicros: Int64
    var isReady: Bool

    static func decodeBSATN(from reader: inout BSATNReader) throws -> PlayerRow {
        return PlayerRow(
            id: try reader.readU64(),
            name: try reader.readString(),
            x: try reader.readFloat(),
            y: try reader.readFloat(),
            health: try reader.readU32(),
            weaponCount: try reader.readU32(),
            kills: try reader.readU32(),
            respawnAtMicros: try reader.readI64(),
            isReady: try reader.readBool()
        )
    }

    func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.appendU64(id)
        try storage.appendString(name)
        storage.appendFloat(x)
        storage.appendFloat(y)
        storage.appendU32(health)
        storage.appendU32(weaponCount)
        storage.appendU32(kills)
        storage.appendI64(respawnAtMicros)
        storage.appendBool(isReady)
    }
}

struct GameState: Codable, Sendable, BSATNSpecialDecodable, BSATNSpecialEncodable {
    var tick: UInt64
    var players: [PlayerRow]
    var mapName: String
    var timeLimit: UInt32

    static func decodeBSATN(from reader: inout BSATNReader) throws -> GameState {
        return GameState(
            tick: try reader.readU64(),
            players: try reader.readArray { reader in try PlayerRow.decodeBSATN(from: &reader) },
            mapName: try reader.readString(),
            timeLimit: try reader.readU32()
        )
    }

    func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.appendU64(tick)
        storage.appendU32(UInt32(players.count))
        for player in players {
            try player.encodeBSATN(to: &storage)
        }
        try storage.appendString(mapName)
        storage.appendU32(timeLimit)
    }
}

// MARK: - Pre-built Test Data

private let samplePoint = Point3D(x: 1.0, y: 2.0, z: 3.0)

private let samplePlayer = PlayerRow(
    id: 12345, name: "BenchmarkPlayer", x: 150.5, y: 200.3,
    health: 100, weaponCount: 3, kills: 42,
    respawnAtMicros: 1_700_000_000_000, isReady: true
)

private let sampleGameState = GameState(
    tick: 99999,
    players: (0..<20).map { i in
        PlayerRow(
            id: UInt64(i), name: "Player \(i)", x: Float(i) * 10, y: Float(i) * 20,
            health: 100, weaponCount: 1, kills: 0,
            respawnAtMicros: 0, isReady: true
        )
    },
    mapName: "benchmark_arena",
    timeLimit: 300
)

private let encodedPoint: Data = try! BSATNEncoder().encode(samplePoint)
private let encodedPlayer: Data = try! BSATNEncoder().encode(samplePlayer)
private let encodedGameState: Data = try! BSATNEncoder().encode(sampleGameState)
private let reducerArgsPayload = Data(repeating: 0x2A, count: 128)
private let procedureArgsPayload = Data(repeating: 0x7F, count: 128)
private let reducerReturnPayload: Data = try! BSATNEncoder().encode(samplePoint)
private let procedureReturnPayload: Data = try! BSATNEncoder().encode(samplePlayer)

private let encodedServerInitialConnection = makeInitialConnectionMessage()
private let encodedServerSubscriptionError = makeSubscriptionErrorMessage()
private let encodedServerTransactionUpdate = makeTransactionUpdateMessage()
private let encodedServerReducerResult = makeReducerResultMessage()
private let encodedServerProcedureResult = makeProcedureResultMessage()

private func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
    var littleEndian = value.littleEndian
    withUnsafeBytes(of: &littleEndian) { bytes in
        data.append(contentsOf: bytes)
    }
}

private func appendBSATNString(_ value: String, to data: inout Data) {
    data.append(try! BSATNEncoder().encode(value))
}

private func appendEmptyRowList(to data: inout Data) {
    // RowSizeHint::RowOffsets([])
    appendLE(UInt8(1), to: &data)
    appendLE(UInt32(0), to: &data)
    appendLE(UInt32(0), to: &data)
}

private func makeInitialConnectionMessage() -> Data {
    var frame = Data([0]) // ServerMessage::InitialConnection
    frame.append(Data(repeating: 0xAB, count: 32)) // identity
    frame.append(Data(repeating: 0xCD, count: 16)) // connection_id
    appendBSATNString("benchmark-token", to: &frame)
    return frame
}

private func makeSubscriptionErrorMessage() -> Data {
    var frame = Data([3]) // ServerMessage::SubscriptionError
    appendLE(UInt8(0), to: &frame) // request_id: Some
    appendLE(UInt32(44), to: &frame) // request_id value
    appendLE(UInt32(9), to: &frame) // query_set_id
    appendBSATNString("bad query", to: &frame)
    return frame
}

private func makeTransactionUpdateMessage() -> Data {
    var frame = Data([4]) // ServerMessage::TransactionUpdate
    appendLE(UInt32(1), to: &frame) // query_sets count

    appendLE(UInt32(7), to: &frame) // query_set_id
    appendLE(UInt32(1), to: &frame) // table updates count

    appendBSATNString("player", to: &frame)
    appendLE(UInt32(1), to: &frame) // row updates count
    appendLE(UInt8(0), to: &frame) // TableUpdateRows::PersistentTable

    // inserts + deletes row lists
    appendEmptyRowList(to: &frame)
    appendEmptyRowList(to: &frame)
    return frame
}

private func makeSubscribeMessage() -> ClientMessage {
    .subscribe(Subscribe(queryStrings: ["SELECT * FROM player"], requestId: RequestId(rawValue: 1), querySetId: QuerySetId(rawValue: 7)))
}

private func makeReducerMessage() -> ClientMessage {
    .callReducer(CallReducer(requestId: RequestId(rawValue: 44), flags: 0, reducer: "move", args: reducerArgsPayload))
}

private func makeProcedureMessage() -> ClientMessage {
    .callProcedure(CallProcedure(requestId: RequestId(rawValue: 45), flags: 0, procedure: "spawn", args: procedureArgsPayload))
}

private func makeReducerResultMessage() -> Data {
    var frame = Data([6]) // ServerMessage::ReducerResult
    appendLE(UInt32(44), to: &frame) // request_id
    appendLE(Int64(1_700_000_000), to: &frame) // timestamp
    appendLE(UInt8(0), to: &frame) // ReducerOutcome::Ok
    appendLE(UInt32(reducerReturnPayload.count), to: &frame) // ret_value length
    frame.append(reducerReturnPayload) // ret_value payload
    appendLE(UInt32(0), to: &frame) // transaction_update.query_sets count
    return frame
}

private func makeProcedureResultMessage() -> Data {
    var frame = Data([7]) // ServerMessage::ProcedureResult
    appendLE(UInt8(0), to: &frame) // ProcedureStatus::Returned
    appendLE(UInt32(procedureReturnPayload.count), to: &frame) // returned bytes length
    frame.append(procedureReturnPayload)
    appendLE(Int64(1_700_000_000), to: &frame) // timestamp
    appendLE(Int64(2_000), to: &frame) // total_host_execution_duration
    appendLE(UInt32(45), to: &frame) // request_id
    return frame
}

let benchmarks: @Sendable () -> Void = {
    Benchmark.defaultConfiguration = .init(
        metrics: [.wallClock, .throughput],
        maxDuration: .seconds(3),
        maxIterations: 1_000_000
    )

    // MARK: - BSATN Encode

    Benchmark("BSATN Encode Point3D") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(samplePoint))
        }
    }

    Benchmark("BSATN Encode PlayerRow") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(samplePlayer))
        }
    }

    Benchmark("BSATN Encode GameState (20 players)") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(sampleGameState))
        }
    }

    // MARK: - BSATN Decode

    Benchmark("BSATN Decode Point3D") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(Point3D.self, from: encodedPoint))
        }
    }

    Benchmark("BSATN Decode PlayerRow") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(PlayerRow.self, from: encodedPlayer))
        }
    }

    Benchmark("BSATN Decode GameState (20 players)") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(GameState.self, from: encodedGameState))
        }
    }

    // MARK: - Message Encode/Decode

    Benchmark("Message Encode Subscribe") { benchmark in
        let encoder = BSATNEncoder()
        let message = makeSubscribeMessage()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(message))
        }
    }

    Benchmark("Message Encode CallReducer (128-byte args)") { benchmark in
        let encoder = BSATNEncoder()
        let message = makeReducerMessage()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(message))
        }
    }

    Benchmark("Message Encode CallProcedure (128-byte args)") { benchmark in
        let encoder = BSATNEncoder()
        let message = makeProcedureMessage()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(message))
        }
    }

    Benchmark("Message Decode InitialConnection") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(ServerMessage.self, from: encodedServerInitialConnection))
        }
    }

    Benchmark("Message Decode SubscriptionError") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(ServerMessage.self, from: encodedServerSubscriptionError))
        }
    }

    Benchmark("Message Decode TransactionUpdate (single table)") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(ServerMessage.self, from: encodedServerTransactionUpdate))
        }
    }

    Benchmark("RoundTrip Reducer (encode request + decode result)") { benchmark in
        let encoder = BSATNEncoder()
        let decoder = BSATNDecoder()
        let payloadDecoder = BSATNDecoder()
        let request = makeReducerMessage()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(request))
            let serverMessage = try decoder.decode(ServerMessage.self, from: encodedServerReducerResult)
            guard case .reducerResult(let reducerResult) = serverMessage else {
                fatalError("Expected reducer result")
            }
            guard case .ok(let ok) = reducerResult.result else {
                fatalError("Expected reducer ok")
            }
            blackHole(try payloadDecoder.decode(Point3D.self, from: ok.retValue))
        }
    }

    Benchmark("RoundTrip Procedure (encode request + decode result)") { benchmark in
        let encoder = BSATNEncoder()
        let decoder = BSATNDecoder()
        let payloadDecoder = BSATNDecoder()
        let request = makeProcedureMessage()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(request))
            let serverMessage = try decoder.decode(ServerMessage.self, from: encodedServerProcedureResult)
            guard case .procedureResult(let procedureResult) = serverMessage else {
                fatalError("Expected procedure result")
            }
            guard case .returned(let returnedData) = procedureResult.status else {
                fatalError("Expected procedure return payload")
            }
            blackHole(try payloadDecoder.decode(PlayerRow.self, from: returnedData))
        }
    }

    // MARK: - Cache Operations

    Benchmark("Cache Insert 100 rows") { benchmark in
        let encoder = BSATNEncoder()
        let rows = (0..<100).map { i in
            try! encoder.encode(Point3D(x: Float(i), y: Float(i), z: Float(i)))
        }
        for _ in benchmark.scaledIterations {
            let cache = TableCache<Point3D>(tableName: "bench")
            for rowBytes in rows {
                try! cache.handleInsert(rowBytes: rowBytes)
            }
            // Performance: We only measure the background insertion/decoding here.
            // In real usage, the UI will sync occasionally.
        }
    }

    Benchmark("Cache Insert 1000 rows") { benchmark in
        let encoder = BSATNEncoder()
        let rows = (0..<1000).map { i in
            try! encoder.encode(Point3D(x: Float(i), y: Float(i), z: Float(i)))
        }
        for _ in benchmark.scaledIterations {
            let cache = TableCache<Point3D>(tableName: "bench")
            for rowBytes in rows {
                try! cache.handleInsert(rowBytes: rowBytes)
            }
        }
    }

    Benchmark("Cache Delete 500 rows from full") { benchmark in
        let encoder = BSATNEncoder()
        let rows = (0..<500).map { i in
            try! encoder.encode(Point3D(x: Float(i), y: Float(i), z: Float(i)))
        }
        for _ in benchmark.scaledIterations {
            let cache = TableCache<Point3D>(tableName: "bench")
            for rowBytes in rows {
                try! cache.handleInsert(rowBytes: rowBytes)
            }
            for rowBytes in rows {
                try! cache.handleDelete(rowBytes: rowBytes)
            }
        }
    }
    
    Benchmark("Cache Sync (1000 rows)") { benchmark in
        let encoder = BSATNEncoder()
        let rows = (0..<1000).map { i in
            try! encoder.encode(Point3D(x: Float(i), y: Float(i), z: Float(i)))
        }
        let cache = TableCache<Point3D>(tableName: "bench")
        for rowBytes in rows {
            try! cache.handleInsert(rowBytes: rowBytes)
        }
        
        for _ in benchmark.scaledIterations {
            await MainActor.run {
                cache.sync()
                blackHole(cache.rows)
            }
        }
    }
}
