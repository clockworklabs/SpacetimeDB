import Foundation

public struct Identity: Codable, Sendable, Hashable {
    public static let byteCount = 32
    public var rawBytes: Data

    public init(rawBytes: Data) {
        self.rawBytes = rawBytes
    }
}

public struct ClientConnectionId: Codable, Sendable, Hashable {
    public static let byteCount = 16
    public var rawBytes: Data

    public init(rawBytes: Data) {
        self.rawBytes = rawBytes
    }
}

public enum ScheduleAt: Codable, Sendable {
    case interval(UInt64)
    case time(UInt64)
}

public enum SpacetimeResult<Ok: Codable & Sendable, Err: Codable & Sendable>: Codable, Sendable {
    case ok(Ok)
    case err(Err)
}

extension Identity: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.rawBytes = try decoder.storage.readBytes(count: Self.byteCount)
    }
}

extension Identity: BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        guard rawBytes.count == Self.byteCount else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        encoder.storage.append(rawBytes)
    }
}

extension ClientConnectionId: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        self.rawBytes = try decoder.storage.readBytes(count: Self.byteCount)
    }
}

extension ClientConnectionId: BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        guard rawBytes.count == Self.byteCount else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        encoder.storage.append(rawBytes)
    }
}

extension ScheduleAt: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            self = .interval(try decoder.storage.read(UInt64.self))
        case 1:
            self = .time(try decoder.storage.read(UInt64.self))
        default:
            throw BSATNDecodingError.invalidType
        }
    }
}

extension ScheduleAt: BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        switch self {
        case .interval(let value):
            encoder.storage.append(0 as UInt8)
            encoder.storage.append(value)
        case .time(let value):
            encoder.storage.append(1 as UInt8)
            encoder.storage.append(value)
        }
    }
}

extension SpacetimeResult: BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        let container = try decoder.singleValueContainer()
        switch tag {
        case 0:
            self = .ok(try container.decode(Ok.self))
        case 1:
            self = .err(try container.decode(Err.self))
        default:
            throw BSATNDecodingError.invalidType
        }
    }
}

extension SpacetimeResult: BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .ok(let value):
            encoder.storage.append(0 as UInt8)
            try container.encode(value)
        case .err(let value):
            encoder.storage.append(1 as UInt8)
            try container.encode(value)
        }
    }
}
