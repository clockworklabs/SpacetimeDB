import Foundation

@_documentation(visibility: internal)
public enum BSATNEncodingError: Error, Equatable, BitwiseCopyable {
    case lengthOutOfRange
}

@_documentation(visibility: internal)
public struct BSATNStorage: ~Copyable {
    public var data: Data
    
    public init(data: Data = Data()) {
        self.data = data
    }

    public mutating func append(_ newBytes: Data) {
        data.append(newBytes)
    }
    
    public mutating func append<T: FixedWidthInteger>(_ value: T) {
        var littleEndian = value.littleEndian
        withUnsafeBytes(of: &littleEndian) { buffer in
            data.append(buffer.bindMemory(to: UInt8.self))
        }
    }

    public mutating func appendU8(_ value: UInt8) {
        data.append(value)
    }

    public mutating func appendU16(_ value: UInt16) {
        var val = value.littleEndian
        withUnsafeBytes(of: &val) { data.append($0.bindMemory(to: UInt8.self)) }
    }

    public mutating func appendU32(_ value: UInt32) {
        var val = value.littleEndian
        withUnsafeBytes(of: &val) { data.append($0.bindMemory(to: UInt8.self)) }
    }

    public mutating func appendU64(_ value: UInt64) {
        var val = value.littleEndian
        withUnsafeBytes(of: &val) { data.append($0.bindMemory(to: UInt8.self)) }
    }

    public mutating func appendI8(_ value: Int8) {
        data.append(UInt8(bitPattern: value))
    }

    public mutating func appendI16(_ value: Int16) {
        var val = value.littleEndian
        withUnsafeBytes(of: &val) { data.append($0.bindMemory(to: UInt8.self)) }
    }

    public mutating func appendI32(_ value: Int32) {
        var val = value.littleEndian
        withUnsafeBytes(of: &val) { data.append($0.bindMemory(to: UInt8.self)) }
    }

    public mutating func appendI64(_ value: Int64) {
        var val = value.littleEndian
        withUnsafeBytes(of: &val) { data.append($0.bindMemory(to: UInt8.self)) }
    }

    public mutating func appendString(_ value: String) throws(BSATNEncodingError) {
        let utf8 = Data(value.utf8)
        guard utf8.count <= Int(UInt32.max) else {
            throw .lengthOutOfRange
        }
        append(UInt32(utf8.count))
        append(utf8)
    }

    public mutating func appendBool(_ value: Bool) {
        append(value ? 1 as UInt8 : 0 as UInt8)
    }

    public mutating func appendFloat(_ value: Float) {
        append(value.bitPattern)
    }


    public mutating func appendDouble(_ value: Double) {
        append(value.bitPattern)
    }

    public mutating func fallbackEncode<T: Encodable>(_ value: T) throws {
        let wrapper = BSATNStorageWrapper(storage: BSATNStorage(data: self.data))
        self.data = Data()
        let encoder = _BSATNEncoder(storage: wrapper, codingPath: [])
        try value.encode(to: encoder)
        self.data = wrapper.storage.data
    }
}

class BSATNStorageWrapper {
    var storage: BSATNStorage
    init(storage: consuming BSATNStorage) {
        self.storage = storage
    }
    
    func withStorage<T>(_ block: (inout BSATNStorage) throws -> T) rethrows -> T {
        try block(&storage)
    }
}

@_documentation(visibility: internal)
public final class BSATNEncoder: Sendable {
    public init() {}
    
    public func encode<T: Encodable>(_ value: T) throws -> Data {
        var storage = BSATNStorage()
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: &storage)
        } else {
            let wrapper = BSATNStorageWrapper(storage: BSATNStorage(data: storage.data))
            storage.data = Data()
            let encoder = _BSATNEncoder(storage: wrapper, codingPath: [])
            try value.encode(to: encoder)
            storage.data = wrapper.storage.data
        }
        return storage.data
    }
}

struct _BSATNEncoder: Encoder {
    var storage: BSATNStorageWrapper
    var codingPath: [CodingKey]
    var userInfo: [CodingUserInfoKey: Any] = [:]
    
    init(storage: BSATNStorageWrapper, codingPath: [CodingKey] = []) {
        self.storage = storage
        self.codingPath = codingPath
    }
    
    var data: Data { storage.withStorage { $0.data } }
    
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
        encoder.storage.withStorage { $0.append(1 as UInt8) }
    }
    
    mutating func encodeIfPresent<T: Encodable>(_ value: T?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            if let bsatnSpecial = value as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: &encoder.storage.storage)
            } else {
                let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
                try value.encode(to: childEncoder)
            }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }
    
    mutating func encodeIfPresent(_ value: Bool?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.appendBool(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: String?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            try encoder.storage.withStorage { try $0.appendString(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Float?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.appendFloat(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Double?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.appendDouble(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Int?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(Int64(value)) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Int8?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Int16?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Int32?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: Int64?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: UInt?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(UInt64(value)) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: UInt8?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: UInt16?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: UInt32?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    mutating func encodeIfPresent(_ value: UInt64?, forKey key: Key) throws {
        if let value = value {
            encoder.storage.withStorage { $0.append(0 as UInt8) }
            encoder.storage.withStorage { $0.append(value) }
        } else {
            encoder.storage.withStorage { $0.append(1 as UInt8) }
        }
    }

    public func encode<T: Encodable>(_ value: T, forKey key: Key) throws {
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: &encoder.storage.storage)
        } else {
            let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath + [key])
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
        encoder.storage.withStorage { $0.append(1 as UInt8) }
        count += 1
    }
    
    mutating func encode<T: Encodable>(_ value: T) throws {
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: &encoder.storage.storage)
        } else {
            let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
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
    
    mutating func encodeNil() throws { encoder.storage.withStorage { $0.append(1 as UInt8) } }
    mutating func encode(_ value: Bool) throws { encoder.storage.withStorage { $0.appendBool(value) } }
    mutating func encode(_ value: String) throws { try encoder.storage.withStorage { try $0.appendString(value) } }
    mutating func encode(_ value: Double) throws { encoder.storage.withStorage { $0.appendDouble(value) } }
    mutating func encode(_ value: Float) throws { encoder.storage.withStorage { $0.appendFloat(value) } }
    mutating func encode(_ value: Int) throws { encoder.storage.withStorage { $0.append(Int64(value)) } }
    mutating func encode(_ value: Int8) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: Int16) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: Int32) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: Int64) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: UInt) throws { encoder.storage.withStorage { $0.append(UInt64(value)) } }
    mutating func encode(_ value: UInt8) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: UInt16) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: UInt32) throws { encoder.storage.withStorage { $0.append(value) } }
    mutating func encode(_ value: UInt64) throws { encoder.storage.withStorage { $0.append(value) } }
    
    mutating func encode<T: Encodable>(_ value: T) throws {
        if let bsatnSpecial = value as? BSATNSpecialEncodable {
            try bsatnSpecial.encodeBSATN(to: &encoder.storage.storage)
        } else {
            let childEncoder = _BSATNEncoder(storage: encoder.storage, codingPath: codingPath)
            try value.encode(to: childEncoder)
        }
    }
}

@_documentation(visibility: internal)
public protocol BSATNSpecialEncodable {
    func encodeBSATN(to storage: inout BSATNStorage) throws
}

extension Array: BSATNSpecialEncodable where Element: Encodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        guard self.count <= Int(UInt32.max) else {
            throw BSATNEncodingError.lengthOutOfRange
        }
        storage.appendU32(UInt32(self.count))
        if self.isEmpty { return }

        
        #if _endian(little)
        if Element.self is any BSATNFastCopyable.Type {
            self.withUnsafeBytes { raw in
                if let base = raw.baseAddress {
                    storage.data.append(base.assumingMemoryBound(to: UInt8.self), count: raw.count)
                }
            }
            return
        }
        #endif

        if Element.self is BSATNSpecialEncodable.Type {
            for element in self {
                try (element as! BSATNSpecialEncodable).encodeBSATN(to: &storage)
            }
            return
        }

        let wrapper = BSATNStorageWrapper(storage: BSATNStorage(data: storage.data))
        storage.data = Data()
        let encoder = _BSATNEncoder(storage: wrapper, codingPath: [])
        for element in self {
            if let bsatnSpecial = element as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: &wrapper.storage)
            } else {
                try element.encode(to: encoder)
            }
        }
        storage.data = wrapper.storage.data
    }
}

extension Optional: BSATNSpecialEncodable where Wrapped: Encodable {
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        switch self {
        case .none:
            storage.append(1 as UInt8)
        case .some(let wrapped):
            storage.append(0 as UInt8)
            if let bsatnSpecial = wrapped as? BSATNSpecialEncodable {
                try bsatnSpecial.encodeBSATN(to: &storage)
            } else {
                let wrapper = BSATNStorageWrapper(storage: BSATNStorage(data: storage.data))
                storage.data = Data()
                let encoder = _BSATNEncoder(storage: wrapper, codingPath: [])
                try wrapped.encode(to: encoder)
                storage.data = wrapper.storage.data
            }
        }
    }
}
