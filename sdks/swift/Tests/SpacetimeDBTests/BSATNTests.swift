import XCTest
@testable import SpacetimeDB

final class BSATNTests: XCTestCase {
    
    struct Person: Codable, Equatable {
        var id: Int32
        var name: String
        var isActive: Bool
        var score: Double
    }
    
    struct Team: Codable, Equatable {
        var name: String
        var members: [Person]
        var maybeScore: Double?
    }

    enum WeaponKind: UInt8, Codable, Equatable {
        case sword = 0
        case shuriken = 1

        init(from decoder: Decoder) throws {
            let container = try decoder.singleValueContainer()
            let tag = try container.decode(UInt8.self)
            guard let value = Self(rawValue: tag) else {
                throw BSATNDecodingError.invalidType
            }
            self = value
        }

        func encode(to encoder: Encoder) throws {
            var container = encoder.singleValueContainer()
            try container.encode(self.rawValue)
        }
    }

    enum CombatEvent: Codable, Equatable {
        case joined(Person)
        case attacked(targetId: UInt32)
        case respawned

        init(from decoder: Decoder) throws {
            let container = try decoder.singleValueContainer()
            let tag = try container.decode(UInt8.self)
            switch tag {
            case 0:
                self = .joined(try container.decode(Person.self))
            case 1:
                self = .attacked(targetId: try container.decode(UInt32.self))
            case 2:
                self = .respawned
            default:
                throw BSATNDecodingError.invalidType
            }
        }

        func encode(to encoder: Encoder) throws {
            var container = encoder.singleValueContainer()
            switch self {
            case .joined(let person):
                try container.encode(UInt8(0))
                try container.encode(person)
            case .attacked(let targetId):
                try container.encode(UInt8(1))
                try container.encode(targetId)
            case .respawned:
                try container.encode(UInt8(2))
            }
        }
    }
    
    func testEncodeDecodePrimitiveString() throws {
        let string = "Hello SpacetimeDB"
        let encoder = BSATNEncoder()
        let data = try encoder.encode(string)
        
        let decoder = BSATNDecoder()
        let decoded = try decoder.decode(String.self, from: data)
        XCTAssertEqual(string, decoded)
        
        // Let's verify exact bytes for string manually just in case
        // Length of string is 17 = 0x11
        // UInt32 length prefix: [0x11, 0x00, 0x00, 0x00]
        let lengthBytes = data.prefix(4)
        XCTAssertEqual(lengthBytes, Data([0x11, 0x00, 0x00, 0x00]))
    }
    
    func testEncodeDecodeStruct() throws {
        let person = Person(id: 42, name: "Alice", isActive: true, score: 3.14)
        
        let data = try BSATNEncoder().encode(person)
        let decoded = try BSATNDecoder().decode(Person.self, from: data)
        
        XCTAssertEqual(person, decoded)
    }
    
    func testEncodeDecodeArrayAndOptional() throws {
        let p1 = Person(id: 1, name: "Bob", isActive: false, score: 0.0)
        let p2 = Person(id: 2, name: "Charlie", isActive: true, score: 100.5)
        
        let team = Team(name: "Winners", members: [p1, p2], maybeScore: 50.0)
        let teamData = try BSATNEncoder().encode(team)
        let decodedTeam = try BSATNDecoder().decode(Team.self, from: teamData)
        XCTAssertEqual(team, decodedTeam)
        
        let teamNoScore = Team(name: "Losers", members: [], maybeScore: nil)
        let dataNoScore = try BSATNEncoder().encode(teamNoScore)
        let decodedNoScore = try BSATNDecoder().decode(Team.self, from: dataNoScore)
        XCTAssertEqual(teamNoScore, decodedNoScore)

        XCTAssertEqual(dataNoScore.count, 15)
        XCTAssertEqual(dataNoScore.last, 0x01) // maybeScore nil = 1 (SpacetimeDB Option::none)
    }

    func testEncodeDecodeOptionalNestedSpecialTypes() throws {
        struct Payload: Codable, Equatable {
            var maybeInts: [Int32]?
        }

        let some = Payload(maybeInts: [10, 20, 30])
        let encodedSome = try BSATNEncoder().encode(some)
        let decodedSome = try BSATNDecoder().decode(Payload.self, from: encodedSome)
        XCTAssertEqual(decodedSome, some)

        let none = Payload(maybeInts: nil)
        let encodedNone = try BSATNEncoder().encode(none)
        let decodedNone = try BSATNDecoder().decode(Payload.self, from: encodedNone)
        XCTAssertEqual(decodedNone, none)
    }

    func testEncodeDecodeSumAndPlainEnums() throws {
        struct Envelope: Codable, Equatable {
            var event: CombatEvent
            var weapon: WeaponKind
        }

        let joined = Envelope(
            event: .joined(Person(id: 7, name: "Ninja", isActive: true, score: 9.5)),
            weapon: .sword
        )
        let attacked = Envelope(event: .attacked(targetId: 99), weapon: .shuriken)
        let respawned = Envelope(event: .respawned, weapon: .sword)

        for value in [joined, attacked, respawned] {
            let encoded = try BSATNEncoder().encode(value)
            let decoded = try BSATNDecoder().decode(Envelope.self, from: encoded)
            XCTAssertEqual(decoded, value)
        }
    }

    func testDecodePlayerRowFromWireSample() throws {
        struct PlayerWireVCurrent: Codable, Equatable {
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

        // Captured from runtime failure log in NinjaGame.
        let rowHex = "d934dcd1373395b609000000506c617965722034330000fa430000fa4364000000000000000000000000000000000000000001"
        let rowData = try XCTUnwrap(Data(hex: rowHex))

        let decoded = try BSATNDecoder().decode(PlayerWireVCurrent.self, from: rowData)
        XCTAssertEqual(decoded.name, "Player 43")
        XCTAssertFalse(decoded.isReady)
        XCTAssertNil(decoded.lobbyId)
    }

    func testDecodeRejectsInvalidBoolByte() {
        let invalidBool = Data([0x02])
        XCTAssertThrowsError(try BSATNDecoder().decode(Bool.self, from: invalidBool)) { error in
            XCTAssertEqual(error as? BSATNDecodingError, .invalidType)
        }
    }

    func testDecodeRejectsInvalidOptionalTag() {
        struct Payload: Codable, Equatable {
            var maybeValue: UInt32?
        }

        // Option tag must be 0 (Some) or 1 (None).
        let invalidTag = Data([0x02])
        XCTAssertThrowsError(try BSATNDecoder().decode(Payload.self, from: invalidTag)) { error in
            XCTAssertEqual(error as? BSATNDecodingError, .invalidType)
        }
    }

    func testEncodeDecodeIdentityAndConnectionId() throws {
        let identityBytes = Data((0..<Identity.byteCount).map { UInt8($0) })
        let identity = Identity(rawBytes: identityBytes)
        let encodedIdentity = try BSATNEncoder().encode(identity)
        XCTAssertEqual(encodedIdentity, identityBytes)
        let decodedIdentity = try BSATNDecoder().decode(Identity.self, from: encodedIdentity)
        XCTAssertEqual(decodedIdentity, identity)

        let connectionBytes = Data((0..<ClientConnectionId.byteCount).map { UInt8(255 - $0) })
        let connectionId = ClientConnectionId(rawBytes: connectionBytes)
        let encodedConnection = try BSATNEncoder().encode(connectionId)
        XCTAssertEqual(encodedConnection, connectionBytes)
        let decodedConnection = try BSATNDecoder().decode(ClientConnectionId.self, from: encodedConnection)
        XCTAssertEqual(decodedConnection, connectionId)
    }

    func testEncodeRejectsWrongSizedIdentityAndConnectionId() {
        XCTAssertThrowsError(try BSATNEncoder().encode(Identity(rawBytes: Data([0x01]))))
        XCTAssertThrowsError(try BSATNEncoder().encode(ClientConnectionId(rawBytes: Data([0x01, 0x02]))))
    }

    func testEncodeDecodeScheduleAt() throws {
        let interval = ScheduleAt.interval(120)
        let encodedInterval = try BSATNEncoder().encode(interval)
        let decodedInterval = try BSATNDecoder().decode(ScheduleAt.self, from: encodedInterval)
        guard case .interval(let value) = decodedInterval else {
            return XCTFail("Expected .interval")
        }
        XCTAssertEqual(value, 120)

        let time = ScheduleAt.time(999)
        let encodedTime = try BSATNEncoder().encode(time)
        let decodedTime = try BSATNDecoder().decode(ScheduleAt.self, from: encodedTime)
        guard case .time(let value) = decodedTime else {
            return XCTFail("Expected .time")
        }
        XCTAssertEqual(value, 999)
    }

    func testEncodeDecodeSpacetimeResultWithNonErrorErrType() throws {
        let ok: SpacetimeResult<UInt32, String> = .ok(7)
        let encodedOk = try BSATNEncoder().encode(ok)
        let decodedOk = try BSATNDecoder().decode(SpacetimeResult<UInt32, String>.self, from: encodedOk)
        guard case .ok(let okValue) = decodedOk else {
            return XCTFail("Expected .ok")
        }
        XCTAssertEqual(okValue, 7)

        let err: SpacetimeResult<UInt32, String> = .err("nope")
        let encodedErr = try BSATNEncoder().encode(err)
        let decodedErr = try BSATNDecoder().decode(SpacetimeResult<UInt32, String>.self, from: encodedErr)
        guard case .err(let errValue) = decodedErr else {
            return XCTFail("Expected .err")
        }
        XCTAssertEqual(errValue, "nope")
    }
}

private extension Data {
    init?(hex: String) {
        let chars = Array(hex)
        guard chars.count % 2 == 0 else { return nil }
        var out = Data(capacity: chars.count / 2)
        var i = 0
        while i < chars.count {
            guard
                let hi = chars[i].hexDigitValue,
                let lo = chars[i + 1].hexDigitValue
            else { return nil }
            out.append(UInt8(hi * 16 + lo))
            i += 2
        }
        self = out
    }
}
