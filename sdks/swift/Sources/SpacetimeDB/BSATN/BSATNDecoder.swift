import Foundation

public enum BSATNDecodingError: Error, Equatable {
    case unexpectedEndOfData
    case invalidStringEncoding
    case invalidType
    case unsupportedType
}

public class BSATNDecoder {
    public init() {}
    
    public func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        let storage = BSATNReader(data: data)
        let decoder = _BSATNDecoder(storage: storage, codingPath: [])
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.init(fromBSATN: decoder) as! T
        }
        return try T(from: decoder)
    }
    
    public func decode(_ type: String.Type, from data: Data) throws -> String {
        let storage = BSATNReader(data: data)
        let decoder = _BSATNDecoder(storage: storage, codingPath: [])
        return try decoder.singleValueContainer().decode(String.self)
    }
}

class BSATNReader {
    let data: Data
    var offset: Int = 0
    
    init(data: Data) {
        self.data = data
    }
    
    var isAtEnd: Bool {
        return offset >= data.count
    }
    
    var remaining: Int {
        return max(0, data.count - offset)
    }
    
    func readBytes(count: Int) throws -> Data {
        guard offset + count <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let bytes = data.subdata(in: offset..<(offset + count))
        offset += count
        return bytes
    }
    
    func read<T: FixedWidthInteger>(_ type: T.Type) throws -> T {
        let size = MemoryLayout<T>.size
        let bytes = try readBytes(count: size)
        let value = bytes.withUnsafeBytes { $0.loadUnaligned(as: T.self) }
        return T(littleEndian: value)
    }
    
    func readDouble() throws -> Double {
        let bitPattern = try read(UInt64.self)
        return Double(bitPattern: bitPattern)
    }
    
    func readFloat() throws -> Float {
        let bitPattern = try read(UInt32.self)
        return Float(bitPattern: bitPattern)
    }
}

struct _BSATNDecoder: Decoder {
    var storage: BSATNReader
    var codingPath: [CodingKey]
    var userInfo: [CodingUserInfoKey: Any] = [:]
    
    func container<Key>(keyedBy type: Key.Type) throws -> KeyedDecodingContainer<Key> where Key : CodingKey {
        return KeyedDecodingContainer(KeyedBSATNDecodingContainer<Key>(decoder: self))
    }
    
    func unkeyedContainer() throws -> UnkeyedDecodingContainer {
        return UnkeyedBSATNDecodingContainer(decoder: self)
    }
    
    func singleValueContainer() throws -> SingleValueDecodingContainer {
        return SingleValueBSATNDecodingContainer(decoder: self)
    }
}

struct KeyedBSATNDecodingContainer<Key: CodingKey>: KeyedDecodingContainerProtocol {
    var decoder: _BSATNDecoder
    var codingPath: [CodingKey] { decoder.codingPath }
    var allKeys: [Key] = []
    
    func contains(_ key: Key) -> Bool {
        return true
    }
    
    func decodeNil(forKey key: Key) throws -> Bool {
        // SpacetimeDB Option encoding: 0 = Some, 1 = None.
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0:
            return false
        case 1:
            return true
        default:
            throw BSATNDecodingError.invalidType
        }
    }
    
    func decode<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T {
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.init(fromBSATN: childDecoder) as! T
        }
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 {
            return nil
        }
        guard tag == 0 else {
            throw BSATNDecodingError.invalidType
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.init(fromBSATN: childDecoder) as? T
        }
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent(_ type: Double.Type, forKey key: Key) throws -> Double? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 {
            return nil
        }
        guard tag == 0 else {
            throw BSATNDecodingError.invalidType
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try Double(from: childDecoder)
    }
    
    func nestedContainer<NestedKey>(keyedBy type: NestedKey.Type, forKey key: Key) throws -> KeyedDecodingContainer<NestedKey> where NestedKey : CodingKey {
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try childDecoder.container(keyedBy: type)
    }
    
    func nestedUnkeyedContainer(forKey key: Key) throws -> UnkeyedDecodingContainer {
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try childDecoder.unkeyedContainer()
    }
    
    func superDecoder() throws -> Decoder {
        return _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
    }
    
    func superDecoder(forKey key: Key) throws -> Decoder {
        return _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
    }
}

struct UnkeyedBSATNDecodingContainer: UnkeyedDecodingContainer {
    var decoder: _BSATNDecoder
    var codingPath: [CodingKey] { decoder.codingPath }
    
    var count: Int? = nil 
    var isAtEnd: Bool {
        return decoder.storage.isAtEnd
    }
    var currentIndex: Int = 0
    
    mutating func decodeNil() throws -> Bool {
        // SpacetimeDB Option encoding: 0 = Some, 1 = None.
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 {
            currentIndex += 1
            return true
        }
        guard tag == 0 else {
            throw BSATNDecodingError.invalidType
        }
        return false
    }
    
    mutating func decode<T: Decodable>(_ type: T.Type) throws -> T {
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
        let value: T
        if let specialType = type as? BSATNSpecialDecodable.Type {
            value = try specialType.init(fromBSATN: childDecoder) as! T
        } else {
            value = try T(from: childDecoder)
        }
        currentIndex += 1
        return value
    }
    
    mutating func nestedContainer<NestedKey>(keyedBy type: NestedKey.Type) throws -> KeyedDecodingContainer<NestedKey> where NestedKey : CodingKey {
        fatalError("Nested containers in unkeyed containers not supported by pure BSATN.")
    }
    
    mutating func nestedUnkeyedContainer() throws -> UnkeyedDecodingContainer {
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
        currentIndex += 1
        return try childDecoder.unkeyedContainer()
    }
    
    mutating func superDecoder() throws -> Decoder {
        return _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
    }
}

struct SingleValueBSATNDecodingContainer: SingleValueDecodingContainer {
    var decoder: _BSATNDecoder
    var codingPath: [CodingKey] { decoder.codingPath }
    
    func decodeNil() -> Bool {
        // SpacetimeDB Option encoding: 0 = Some, 1 = None.
        guard let tag = try? decoder.storage.read(UInt8.self) else {
            return false
        }
        return tag == 1
    }
    
    func decode(_ type: Bool.Type) throws -> Bool {
        let byte = try decoder.storage.read(UInt8.self)
        switch byte {
        case 0:
            return false
        case 1:
            return true
        default:
            throw BSATNDecodingError.invalidType
        }
    }
    
    func decode(_ type: String.Type) throws -> String {
        let length = try decoder.storage.read(UInt32.self)
        let bytes = try decoder.storage.readBytes(count: Int(length))
        guard let string = String(data: bytes, encoding: .utf8) else {
            throw BSATNDecodingError.invalidStringEncoding
        }
        return string
    }
    
    func decode(_ type: Double.Type) throws -> Double { return try decoder.storage.readDouble() }
    func decode(_ type: Float.Type) throws -> Float { return try decoder.storage.readFloat() }
    func decode(_ type: Int.Type) throws -> Int { return Int(try decoder.storage.read(Int64.self)) }
    func decode(_ type: Int8.Type) throws -> Int8 { return try decoder.storage.read(Int8.self) }
    func decode(_ type: Int16.Type) throws -> Int16 { return try decoder.storage.read(Int16.self) }
    func decode(_ type: Int32.Type) throws -> Int32 { return try decoder.storage.read(Int32.self) }
    func decode(_ type: Int64.Type) throws -> Int64 { return try decoder.storage.read(Int64.self) }
    func decode(_ type: UInt.Type) throws -> UInt { return UInt(try decoder.storage.read(UInt64.self)) }
    func decode(_ type: UInt8.Type) throws -> UInt8 { return try decoder.storage.read(UInt8.self) }
    func decode(_ type: UInt16.Type) throws -> UInt16 { return try decoder.storage.read(UInt16.self) }
    func decode(_ type: UInt32.Type) throws -> UInt32 { return try decoder.storage.read(UInt32.self) }
    func decode(_ type: UInt64.Type) throws -> UInt64 { return try decoder.storage.read(UInt64.self) }
    
    func decode<T: Decodable>(_ type: T.Type) throws -> T {
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.init(fromBSATN: decoder) as! T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
        return try T(from: childDecoder)
    }
}

protocol BSATNSpecialDecodable {
    init(fromBSATN decoder: _BSATNDecoder) throws
}

extension Array: BSATNSpecialDecodable where Element: Decodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let length = try decoder.storage.read(UInt32.self)
        self = []
        self.reserveCapacity(Int(length))
        var container = try decoder.unkeyedContainer()
        for _ in 0..<length {
            self.append(try container.decode(Element.self))
        }
    }
}

extension Optional: BSATNSpecialDecodable where Wrapped: Decodable {
    init(fromBSATN decoder: _BSATNDecoder) throws {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 {
            self = .none
        } else if tag == 0 {
            if let specialType = Wrapped.self as? BSATNSpecialDecodable.Type {
                self = .some(try specialType.init(fromBSATN: decoder) as! Wrapped)
            } else {
                self = .some(try Wrapped(from: decoder))
            }
        } else {
            throw BSATNDecodingError.invalidType
        }
    }
}
