import Foundation

public enum BSATNEncodingError: Error {
    case lengthOutOfRange
}

public class BSATNEncoder {
    public init() {}
    
    public func encode<T: Encodable>(_ value: T) throws -> Data {
        let storage = BSATNStorage()
        let encoder = _BSATNEncoder(storage: storage, codingPath: [])
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: encoder)
        } else {
            try value.encode(to: encoder)
        }
        return storage.data
    }
}

class BSATNStorage {
    var data = Data()
    
    func append(_ newBytes: Data) {
        data.append(newBytes)
    }
    
    func append<T: FixedWidthInteger>(_ value: T) {
        var littleEndian = value.littleEndian
        let bytes = withUnsafeBytes(of: &littleEndian) { Data($0) }
        data.append(bytes)
    }
}

struct _BSATNEncoder: Encoder {
    var storage: BSATNStorage
    var codingPath: [CodingKey]
    var userInfo: [CodingUserInfoKey: Any] = [:]
    
    init(storage: BSATNStorage = BSATNStorage(), codingPath: [CodingKey] = []) {
        self.storage = storage
        self.codingPath = codingPath
    }
    
    var data: Data { storage.data }
    
    func container<Key>(keyedBy type: Key.Type) -> KeyedEncodingContainer<Key> where Key : CodingKey {
        return KeyedEncodingContainer(KeyedBSATNEncodingContainer<Key>(encoder: self))
    }
    
    func unkeyedContainer() -> UnkeyedEncodingContainer {
        return UnkeyedBSATNEncodingContainer(encoder: self)
    }
    
    func singleValueContainer() -> SingleValueEncodingContainer {
        return SingleValueBSATNEncodingContainer(encoder: self)
    }
}

struct KeyedBSATNEncodingContainer<Key: CodingKey>: KeyedEncodingContainerProtocol {
    var encoder: _BSATNEncoder
    var codingPath: [CodingKey] { encoder.codingPath }
    
    mutating func encodeNil(forKey key: Key) throws {
        encoder.storage.append(1 as UInt8)
    }
    
    mutating func encodeIfPresent<T: Encodable>(_ value: T?, forKey key: Key) throws {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
        if let value = value {
            encoder.storage.append(0 as UInt8)
            if let bsatnSpecial = value as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: childEncoder)
            } else {
                try value.encode(to: childEncoder)
            }
        } else {
            encoder.storage.append(1 as UInt8)
        }
    }
    
    mutating func encodeIfPresent(_ value: Double?, forKey key: Key) throws {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
        if let value = value {
            encoder.storage.append(0 as UInt8)
            try value.encode(to: childEncoder)
        } else {
            encoder.storage.append(1 as UInt8)
        }
    }
    
    public func encode<T: Encodable>(_ value: T, forKey key: Key) throws {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: childEncoder)
        } else {
            try value.encode(to: childEncoder)
        }
    }
    
    mutating func encodeConditional<T: AnyObject & Encodable>(_ object: T, forKey key: Key) throws {
        try encode(object, forKey: key)
    }
    
    mutating func nestedContainer<NestedKey>(keyedBy keyType: NestedKey.Type, forKey key: Key) -> KeyedEncodingContainer<NestedKey> where NestedKey : CodingKey {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
        return childEncoder.container(keyedBy: keyType)
    }
    
    mutating func nestedUnkeyedContainer(forKey key: Key) -> UnkeyedEncodingContainer {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
        return childEncoder.unkeyedContainer()
    }
    
    mutating func superEncoder() -> Encoder {
        return _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
    }
    
    mutating func superEncoder(forKey key: Key) -> Encoder {
        return _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
    }
}

struct UnkeyedBSATNEncodingContainer: UnkeyedEncodingContainer {
    var encoder: _BSATNEncoder
    var codingPath: [CodingKey] { encoder.codingPath }
    var count: Int = 0
    
    mutating func encodeNil() throws {
        encoder.storage.append(1 as UInt8)
        count += 1
    }
    
    mutating func encode<T: Encodable>(_ value: T) throws {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: childEncoder)
        } else {
            try value.encode(to: childEncoder)
        }
        count += 1
    }
    
    mutating func nestedContainer<NestedKey>(keyedBy keyType: NestedKey.Type) -> KeyedEncodingContainer<NestedKey> where NestedKey : CodingKey {
        fatalError("Nested containers in unkeyed containers not supported by pure BSATN.")
    }
    
    mutating func nestedUnkeyedContainer() -> UnkeyedEncodingContainer {
        let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
        count += 1
        return childEncoder.unkeyedContainer()
    }
    
    mutating func superEncoder() -> Encoder {
        return _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
    }
}

struct SingleValueBSATNEncodingContainer: SingleValueEncodingContainer {
    var encoder: _BSATNEncoder
    var codingPath: [CodingKey] { encoder.codingPath }
    
    mutating func encodeNil() throws {
        encoder.storage.append(1 as UInt8)
    }
    
    mutating func encode(_ value: Bool) throws {
        encoder.storage.append(value ? 1 as UInt8 : 0 as UInt8)
    }
    
    mutating func encode(_ value: String) throws {
        let utf8 = Data(value.utf8)
        guard utf8.count <= Int(UInt32.max) else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        encoder.storage.append(UInt32(utf8.count))
        encoder.storage.append(utf8)
    }
    
    mutating func encode(_ value: Double) throws {
        encoder.storage.append(value.bitPattern)
    }
    
    mutating func encode(_ value: Float) throws {
        encoder.storage.append(value.bitPattern)
    }
    
    mutating func encode(_ value: Int) throws {
        // BSATN doesn't natively use Int. Swift Int depends on arch. Encode as Int64 to be safe?
        // Actually, SpacetimeDB generates specific strictly-typed models.
        // It's safer to let the user types explicitly declare Int32 / Int64.
        encoder.storage.append(Int64(value))
    }
    
    mutating func encode(_ value: Int8) throws { encoder.storage.append(value) }
    mutating func encode(_ value: Int16) throws { encoder.storage.append(value) }
    mutating func encode(_ value: Int32) throws { encoder.storage.append(value) }
    mutating func encode(_ value: Int64) throws { encoder.storage.append(value) }
    mutating func encode(_ value: UInt) throws { encoder.storage.append(UInt64(value)) }
    mutating func encode(_ value: UInt8) throws { encoder.storage.append(value) }
    mutating func encode(_ value: UInt16) throws { encoder.storage.append(value) }
    mutating func encode(_ value: UInt32) throws { encoder.storage.append(value) }
    mutating func encode(_ value: UInt64) throws { encoder.storage.append(value) }
    
    mutating func encode<T: Encodable>(_ value: T) throws {
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: encoder)
        } else {
            let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
            try value.encode(to: childEncoder)
        }
    }
}

protocol BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws
}

extension Array: BSATNSpecialEncodable where Element: Encodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        guard self.count <= Int(UInt32.max) else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        encoder.storage.append(UInt32(self.count))
        var container = encoder.unkeyedContainer()
        for element in self {
            try container.encode(element)
        }
    }
}

extension Optional: BSATNSpecialEncodable where Wrapped: Encodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        switch self {
        case .none:
            encoder.storage.append(1 as UInt8)
        case .some(let wrapped):
            encoder.storage.append(0 as UInt8)
            if let bsatnSpecial = wrapped as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: encoder)
            } else {
                try wrapped.encode(to: encoder)
            }
        }
    }
}
