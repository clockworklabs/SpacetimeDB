import Benchmark
import Foundation
import SpacetimeDB

private struct GeneratedPlayerCodableOnly: Codable, Sendable {
    var id: UInt64
    var name: String
    var x: Float
    var y: Float
    var health: UInt32
    var weaponCount: UInt32
    var kills: UInt32
    var respawnAtMicros: Int64
    var isReady: Bool
    var lobbyId: UInt64?
}

private struct GeneratedPlayerSpecial: Codable, Sendable, BSATNSpecialDecodable, BSATNSpecialEncodable {
    var id: UInt64
    var name: String
    var x: Float
    var y: Float
    var health: UInt32
    var weaponCount: UInt32
    var kills: UInt32
    var respawnAtMicros: Int64
    var isReady: Bool
    var lobbyId: UInt64?

    static func decodeBSATN(from reader: inout BSATNReader) throws -> GeneratedPlayerSpecial {
        GeneratedPlayerSpecial(
            id: try reader.readU64(),
            name: try reader.readString(),
            x: try reader.readFloat(),
            y: try reader.readFloat(),
            health: try reader.readU32(),
            weaponCount: try reader.readU32(),
            kills: try reader.readU32(),
            respawnAtMicros: try reader.readI64(),
            isReady: try reader.readBool(),
            lobbyId: try Optional<UInt64>.decodeBSATN(from: &reader)
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
        try lobbyId.encodeBSATN(to: &storage)
    }
}

private struct ReducerArgsCodableOnly: Codable, Sendable {
    var targetId: UInt64
    var x: Float
    var y: Float
    var weaponSlot: UInt8
}

private struct ReducerArgsSpecial: Codable, Sendable, BSATNSpecialEncodable {
    var targetId: UInt64
    var x: Float
    var y: Float
    var weaponSlot: UInt8

    func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.appendU64(targetId)
        storage.appendFloat(x)
        storage.appendFloat(y)
        storage.appendU8(weaponSlot)
    }
}

private let samplePlayer = GeneratedPlayerSpecial(
    id: 42,
    name: "PerfPlayer",
    x: 123.45,
    y: 678.9,
    health: 99,
    weaponCount: 2,
    kills: 7,
    respawnAtMicros: 1_700_000_000,
    isReady: true,
    lobbyId: 777
)

private let samplePlayerCodable = GeneratedPlayerCodableOnly(
    id: samplePlayer.id,
    name: samplePlayer.name,
    x: samplePlayer.x,
    y: samplePlayer.y,
    health: samplePlayer.health,
    weaponCount: samplePlayer.weaponCount,
    kills: samplePlayer.kills,
    respawnAtMicros: samplePlayer.respawnAtMicros,
    isReady: samplePlayer.isReady,
    lobbyId: samplePlayer.lobbyId
)

private let encodedPlayerSpecial = try! BSATNEncoder().encode(samplePlayer)
private let encodedPlayerCodable = try! BSATNEncoder().encode(samplePlayerCodable)

private let sampleReducerArgsCodable = ReducerArgsCodableOnly(targetId: 100, x: 1.0, y: 2.0, weaponSlot: 3)
private let sampleReducerArgsSpecial = ReducerArgsSpecial(targetId: 100, x: 1.0, y: 2.0, weaponSlot: 3)

private let cacheRowsSpecial: [Data] = {
    let encoder = BSATNEncoder()
    return (0..<1000).map { i in
        let row = GeneratedPlayerSpecial(
            id: UInt64(i),
            name: "P\(i)",
            x: Float(i),
            y: Float(i) * 2,
            health: 100,
            weaponCount: 1,
            kills: 0,
            respawnAtMicros: 0,
            isReady: true,
            lobbyId: UInt64(i % 8)
        )
        return try! encoder.encode(row)
    }
}()

private let cacheRowsCodable: [Data] = {
    let encoder = BSATNEncoder()
    return cacheRowsSpecial.enumerated().map { i, _ in
        let row = GeneratedPlayerCodableOnly(
            id: UInt64(i),
            name: "P\(i)",
            x: Float(i),
            y: Float(i) * 2,
            health: 100,
            weaponCount: 1,
            kills: 0,
            respawnAtMicros: 0,
            isReady: true,
            lobbyId: UInt64(i % 8)
        )
        return try! encoder.encode(row)
    }
}()

let benchmarks: @Sendable () -> Void = {
    Benchmark.defaultConfiguration = .init(
        metrics: [.wallClock, .throughput],
        maxDuration: .seconds(3),
        maxIterations: 1_000_000
    )

    Benchmark("Generated Encode Row (Codable)") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(samplePlayerCodable))
        }
    }

    Benchmark("Generated Encode Row (BSATNSpecial)") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(samplePlayer))
        }
    }

    Benchmark("Generated Decode Row (Codable)") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(GeneratedPlayerCodableOnly.self, from: encodedPlayerCodable))
        }
    }

    Benchmark("Generated Decode Row (BSATNSpecial)") { benchmark in
        let decoder = BSATNDecoder()
        for _ in benchmark.scaledIterations {
            blackHole(try decoder.decode(GeneratedPlayerSpecial.self, from: encodedPlayerSpecial))
        }
    }

    Benchmark("Generated ReducerArgs Encode (Codable)") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(sampleReducerArgsCodable))
        }
    }

    Benchmark("Generated ReducerArgs Encode (BSATNSpecial)") { benchmark in
        let encoder = BSATNEncoder()
        for _ in benchmark.scaledIterations {
            blackHole(try encoder.encode(sampleReducerArgsSpecial))
        }
    }

    Benchmark("Generated Cache Insert 1000 rows (Codable)") { benchmark in
        for _ in benchmark.scaledIterations {
            let cache = TableCache<GeneratedPlayerCodableOnly>(tableName: "generated.players.codable")
            for rowBytes in cacheRowsCodable {
                try! cache.handleInsert(rowBytes: rowBytes)
            }
        }
    }

    Benchmark("Generated Cache Insert 1000 rows (BSATNSpecial)") { benchmark in
        for _ in benchmark.scaledIterations {
            let cache = TableCache<GeneratedPlayerSpecial>(tableName: "generated.players.special")
            for rowBytes in cacheRowsSpecial {
                try! cache.handleInsert(rowBytes: rowBytes)
            }
        }
    }
}
