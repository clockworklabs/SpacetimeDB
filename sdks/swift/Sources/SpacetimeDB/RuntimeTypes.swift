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

public struct QuerySetId: Codable, Sendable, Hashable, RawRepresentable {
    public var rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }
}

public struct RequestId: Codable, Sendable, Hashable, RawRepresentable {
    public var rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }
}

public struct RawIdentifier: Codable, Sendable, Hashable, RawRepresentable {
    public var rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

public struct TimeDurationMicros: Codable, Sendable, Hashable, RawRepresentable {
    public var rawValue: UInt64
    public init(rawValue: UInt64) { self.rawValue = rawValue }
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
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> Identity {
        return Identity(rawBytes: try reader.readBytes(count: Self.byteCount))
    }
}

extension Identity: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        guard rawBytes.count == Self.byteCount else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        storage.append(rawBytes)
    }
}

extension ClientConnectionId: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ClientConnectionId {
        return ClientConnectionId(rawBytes: try reader.readBytes(count: Self.byteCount))
    }
}

extension ClientConnectionId: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        guard rawBytes.count == Self.byteCount else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        storage.append(rawBytes)
    }
}

extension QuerySetId: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> QuerySetId {
        return QuerySetId(rawValue: try reader.read(UInt32.self))
    }
}

extension QuerySetId: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.append(rawValue)
    }
}

extension RequestId: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> RequestId {
        return RequestId(rawValue: try reader.read(UInt32.self))
    }
}

extension RequestId: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.append(rawValue)
    }
}

extension RawIdentifier: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> RawIdentifier {
        return RawIdentifier(rawValue: try reader.readString())
    }
}

extension RawIdentifier: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        try storage.appendString(rawValue)
    }
}

extension TimeDurationMicros: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> TimeDurationMicros {
        return TimeDurationMicros(rawValue: try reader.read(UInt64.self))
    }
}

extension TimeDurationMicros: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        storage.append(rawValue)
    }
}

extension ScheduleAt: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> ScheduleAt {
        let tag = try reader.read(UInt8.self)
        switch tag {
        case 0:
            return .interval(try reader.read(UInt64.self))
        case 1:
            return .time(try reader.read(UInt64.self))
        default:
            throw BSATNDecodingError.invalidType
        }
    }
}

extension ScheduleAt: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        switch self {
        case .interval(let value):
            storage.append(0 as UInt8)
            storage.append(value)
        case .time(let value):
            storage.append(1 as UInt8)
            storage.append(value)
        }
    }
}

extension SpacetimeResult: BSATNSpecialDecodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> SpacetimeResult {
        let tag = try reader.read(UInt8.self)
        switch tag {
        case 0:
            if let specialType = Ok.self as? BSATNSpecialDecodable.Type {
                return .ok(try specialType.decodeBSATN(from: &reader) as! Ok)
            }
            return .ok(try reader.fallbackDecode(Ok.self))
        case 1:
            if let specialType = Err.self as? BSATNSpecialDecodable.Type {
                return .err(try specialType.decodeBSATN(from: &reader) as! Err)
            }
            return .err(try reader.fallbackDecode(Err.self))
        default:
            throw BSATNDecodingError.invalidType
        }
    }
}

extension SpacetimeResult: BSATNSpecialEncodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        switch self {
        case .ok(let value):
            storage.append(0 as UInt8)
            if let bsatnSpecial = value as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: &storage)
            } else {
                try storage.fallbackEncode(value)
            }
        case .err(let value):
            storage.append(1 as UInt8)
            if let bsatnSpecial = value as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: &storage)
            } else {
                try storage.fallbackEncode(value)
            }
        }
    }
}
