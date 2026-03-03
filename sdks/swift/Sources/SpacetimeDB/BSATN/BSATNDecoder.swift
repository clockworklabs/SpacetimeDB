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
        let reader = BSATNReader(data: data)
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.decodeBSATN(from: reader) as! T
        }
        let decoder = _BSATNDecoder(storage: reader, codingPath: [])
        return try T(from: decoder)
    }
    
    public func decode(_ type: String.Type, from data: Data) throws -> String {
        let reader = BSATNReader(data: data)
        return try reader.readString()
    }
}

public class BSATNReader {
    public let data: Data
    public var offset: Int = 0
    
    public init(data: Data) {
        self.data = data
    }
    
    public var isAtEnd: Bool {
        return offset >= data.count
    }
    
    public var remaining: Int {
        return max(0, data.count - offset)
    }
    
    public func readBytes(count: Int) throws -> Data {
        guard offset + count <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let bytes = data.subdata(in: offset..<(offset + count))
        offset += count
        return bytes
    }
    
    public func read<T: FixedWidthInteger>(_ type: T.Type) throws -> T {
        let size = MemoryLayout<T>.size
        guard offset + size <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let value = data.withUnsafeBytes { buffer in
            buffer.loadUnaligned(fromByteOffset: offset, as: T.self)
        }
        offset += size
        return T(littleEndian: value)
    }

    public func readU8() throws -> UInt8 {
        guard offset + 1 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data[offset]
        offset += 1
        return val
    }

    public func readU16() throws -> UInt16 {
        guard offset + 2 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: UInt16.self) }
        offset += 2
        return UInt16(littleEndian: val)
    }

    public func readU32() throws -> UInt32 {
        guard offset + 4 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: UInt32.self) }
        offset += 4
        return UInt32(littleEndian: val)
    }

    public func readU64() throws -> UInt64 {
        guard offset + 8 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: UInt64.self) }
        offset += 8
        return UInt64(littleEndian: val)
    }

    public func readI8() throws -> Int8 {
        guard offset + 1 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = Int8(bitPattern: data[offset])
        offset += 1
        return val
    }

    public func readI16() throws -> Int16 {
        guard offset + 2 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: Int16.self) }
        offset += 2
        return Int16(littleEndian: val)
    }

    public func readI32() throws -> Int32 {
        guard offset + 4 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: Int32.self) }
        offset += 4
        return Int32(littleEndian: val)
    }

    public func readI64() throws -> Int64 {
        guard offset + 8 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: Int64.self) }
        offset += 8
        return Int64(littleEndian: val)
    }
    
    public func readDouble() throws -> Double {
        guard offset + 8 <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let bits = data.withUnsafeBytes { buffer in
            buffer.loadUnaligned(fromByteOffset: offset, as: UInt64.self)
        }
        offset += 8
        return Double(bitPattern: UInt64(littleEndian: bits))
    }

    public func readFloat() throws -> Float {
        guard offset + 4 <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let bits = data.withUnsafeBytes { buffer in
            buffer.loadUnaligned(fromByteOffset: offset, as: UInt32.self)
        }
        offset += 4
        return Float(bitPattern: UInt32(littleEndian: bits))
    }

    public func readString() throws -> String {
        let length = Int(try read(UInt32.self))
        guard offset + length <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let string = data.withUnsafeBytes { buffer in
            let ptr = buffer.baseAddress!.advanced(by: offset).assumingMemoryBound(to: UInt8.self)
            return String(unsafeUninitializedCapacity: length) { dest in
                _ = UnsafeMutableBufferPointer(start: dest.baseAddress!, count: length)
                    .initialize(from: UnsafeBufferPointer(start: ptr, count: length))
                return length
            }
        }
        offset += length
        return string
    }

    public func readBool() throws -> Bool {
        let byte = try read(UInt8.self)
        switch byte {
        case 0: return false
        case 1: return true
        default: throw BSATNDecodingError.invalidType
        }
    }

    public func readArray<T>(_ block: () throws -> T) throws -> [T] {
        let count = try read(UInt32.self)
        var elements: [T] = []
        elements.reserveCapacity(Int(count))
        for _ in 0..<count {
            elements.append(try block())
        }
        return elements
    }

    public func readTaggedEnum<T>(_ block: (UInt8) throws -> T) throws -> T {
        let tag = try read(UInt8.self)
        return try block(tag)
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
        let tag = try decoder.storage.read(UInt8.self)
        switch tag {
        case 0: return false
        case 1: return true
        default: throw BSATNDecodingError.invalidType
        }
    }
    
    func decode<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T {
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.decodeBSATN(from: decoder.storage) as! T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.decodeBSATN(from: decoder.storage) as? T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent(_ type: Bool.Type, forKey key: Key) throws -> Bool? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readBool()
    }

    func decodeIfPresent(_ type: String.Type, forKey key: Key) throws -> String? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readString()
    }

    func decodeIfPresent(_ type: Float.Type, forKey key: Key) throws -> Float? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readFloat()
    }

    func decodeIfPresent(_ type: Double.Type, forKey key: Key) throws -> Double? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readDouble()
    }

    func decodeIfPresent(_ type: Int.Type, forKey key: Key) throws -> Int? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return Int(try decoder.storage.read(Int64.self))
    }

    func decodeIfPresent(_ type: Int8.Type, forKey key: Key) throws -> Int8? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(Int8.self)
    }

    func decodeIfPresent(_ type: Int16.Type, forKey key: Key) throws -> Int16? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(Int16.self)
    }

    func decodeIfPresent(_ type: Int32.Type, forKey key: Key) throws -> Int32? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(Int32.self)
    }

    func decodeIfPresent(_ type: Int64.Type, forKey key: Key) throws -> Int64? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(Int64.self)
    }

    func decodeIfPresent(_ type: UInt.Type, forKey key: Key) throws -> UInt? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return UInt(try decoder.storage.read(UInt64.self))
    }

    func decodeIfPresent(_ type: UInt8.Type, forKey key: Key) throws -> UInt8? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(UInt8.self)
    }

    func decodeIfPresent(_ type: UInt16.Type, forKey key: Key) throws -> UInt16? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(UInt16.self)
    }

    func decodeIfPresent(_ type: UInt32.Type, forKey key: Key) throws -> UInt32? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(UInt32.self)
    }

    func decodeIfPresent(_ type: UInt64.Type, forKey key: Key) throws -> UInt64? {
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.read(UInt64.self)
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
        let tag = try decoder.storage.read(UInt8.self)
        if tag == 1 {
            currentIndex += 1
            return true
        }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return false
    }
    
    mutating func decode<T: Decodable>(_ type: T.Type) throws -> T {
        let value: T
        if let specialType = type as? BSATNSpecialDecodable.Type {
            value = try specialType.decodeBSATN(from: decoder.storage) as! T
        } else {
            let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
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
        guard let tag = try? decoder.storage.read(UInt8.self) else { return false }
        return tag == 1
    }
    
    func decode(_ type: Bool.Type) throws -> Bool { return try decoder.storage.readBool() }
    func decode(_ type: String.Type) throws -> String { return try decoder.storage.readString() }
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
            return try specialType.decodeBSATN(from: decoder.storage) as! T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
        return try T(from: childDecoder)
    }
}

public protocol BSATNSpecialDecodable {
    static func decodeBSATN(from reader: BSATNReader) throws -> Self
}

extension Array: BSATNSpecialDecodable where Element: Decodable {
    public static func decodeBSATN(from reader: BSATNReader) throws -> Array {
        return try reader.readArray {
            if let specialType = Element.self as? BSATNSpecialDecodable.Type {
                return try specialType.decodeBSATN(from: reader) as! Element
            }
            let decoder = _BSATNDecoder(storage: reader, codingPath: [])
            return try Element(from: decoder)
        }
    }
}

extension Optional: BSATNSpecialDecodable where Wrapped: Decodable {
    public static func decodeBSATN(from reader: BSATNReader) throws -> Optional {
        let tag = try reader.read(UInt8.self)
        if tag == 1 {
            return .none
        } else if tag == 0 {
            if let specialType = Wrapped.self as? BSATNSpecialDecodable.Type {
                return .some(try specialType.decodeBSATN(from: reader) as! Wrapped)
            }
            let decoder = _BSATNDecoder(storage: reader, codingPath: [])
            return .some(try Wrapped(from: decoder))
        } else {
            throw BSATNDecodingError.invalidType
        }
    }
}
