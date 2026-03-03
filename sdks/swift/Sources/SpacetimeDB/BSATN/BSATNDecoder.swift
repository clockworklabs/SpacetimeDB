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
    private let nsData: NSData
    private let rawBytes: UnsafeRawPointer
    public var offset: Int = 0
    
    public init(data: Data) {
        self.data = data
        self.nsData = data as NSData
        self.rawBytes = nsData.bytes
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
        let value = rawBytes.loadUnaligned(fromByteOffset: offset, as: T.self)
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
        let val = rawBytes.loadUnaligned(fromByteOffset: offset, as: UInt16.self)
        offset += 2
        return UInt16(littleEndian: val)
    }

    public func readU32() throws -> UInt32 {
        guard offset + 4 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = rawBytes.loadUnaligned(fromByteOffset: offset, as: UInt32.self)
        offset += 4
        return UInt32(littleEndian: val)
    }

    public func readU64() throws -> UInt64 {
        guard offset + 8 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = rawBytes.loadUnaligned(fromByteOffset: offset, as: UInt64.self)
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
        let val = rawBytes.loadUnaligned(fromByteOffset: offset, as: Int16.self)
        offset += 2
        return Int16(littleEndian: val)
    }

    public func readI32() throws -> Int32 {
        guard offset + 4 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = rawBytes.loadUnaligned(fromByteOffset: offset, as: Int32.self)
        offset += 4
        return Int32(littleEndian: val)
    }

    public func readI64() throws -> Int64 {
        guard offset + 8 <= data.count else { throw BSATNDecodingError.unexpectedEndOfData }
        let val = rawBytes.loadUnaligned(fromByteOffset: offset, as: Int64.self)
        offset += 8
        return Int64(littleEndian: val)
    }
    
    public func readDouble() throws -> Double {
        guard offset + 8 <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let bits = rawBytes.loadUnaligned(fromByteOffset: offset, as: UInt64.self)
        offset += 8
        return Double(bitPattern: UInt64(littleEndian: bits))
    }

    public func readFloat() throws -> Float {
        guard offset + 4 <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let bits = rawBytes.loadUnaligned(fromByteOffset: offset, as: UInt32.self)
        offset += 4
        return Float(bitPattern: UInt32(littleEndian: bits))
    }

    public func readString() throws -> String {
        let length = Int(try readU32())
        guard offset + length <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let stringStart = rawBytes.advanced(by: offset).assumingMemoryBound(to: UInt8.self)
        let string = String(decoding: UnsafeBufferPointer(start: stringStart, count: length), as: UTF8.self)
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
        let count = try readU32()
        var elements: [T] = []
        elements.reserveCapacity(Int(count))
        for _ in 0..<count {
            elements.append(try block())
        }
        return elements
    }

    public func readTaggedEnum<T>(_ block: (UInt8) throws -> T) throws -> T {
        let tag = try readU8()
        return try block(tag)
    }

    public func readPrimitiveArray<T: FixedWidthInteger>(_ type: T.Type, count: Int) throws -> [T] {
        let stride = MemoryLayout<T>.stride
        let byteCount = count * stride
        guard offset + byteCount <= data.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        if count == 0 { return [] }

        let src = rawBytes.advanced(by: offset)
        let values: [T] = Array(unsafeUninitializedCapacity: count) { buffer, initializedCount in
            memcpy(buffer.baseAddress!, src, byteCount)
            initializedCount = count
        }
        offset += byteCount

#if _endian(little)
        return values
#else
        var converted = values
        for i in converted.indices {
            converted[i] = T(littleEndian: converted[i])
        }
        return converted
#endif
    }

    public func readFloatArray(count: Int) throws -> [Float] {
        let bits = try readPrimitiveArray(UInt32.self, count: count)
        var values = Array<Float>()
        values.reserveCapacity(bits.count)
        for bitPattern in bits {
            values.append(Float(bitPattern: bitPattern))
        }
        return values
    }

    public func readDoubleArray(count: Int) throws -> [Double] {
        let bits = try readPrimitiveArray(UInt64.self, count: count)
        var values = Array<Double>()
        values.reserveCapacity(bits.count)
        for bitPattern in bits {
            values.append(Double(bitPattern: bitPattern))
        }
        return values
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
        let tag = try decoder.storage.readU8()
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
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try specialType.decodeBSATN(from: decoder.storage) as? T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent(_ type: Bool.Type, forKey key: Key) throws -> Bool? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readBool()
    }

    func decodeIfPresent(_ type: String.Type, forKey key: Key) throws -> String? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readString()
    }

    func decodeIfPresent(_ type: Float.Type, forKey key: Key) throws -> Float? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readFloat()
    }

    func decodeIfPresent(_ type: Double.Type, forKey key: Key) throws -> Double? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readDouble()
    }

    func decodeIfPresent(_ type: Int.Type, forKey key: Key) throws -> Int? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return Int(try decoder.storage.readI64())
    }

    func decodeIfPresent(_ type: Int8.Type, forKey key: Key) throws -> Int8? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readI8()
    }

    func decodeIfPresent(_ type: Int16.Type, forKey key: Key) throws -> Int16? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readI16()
    }

    func decodeIfPresent(_ type: Int32.Type, forKey key: Key) throws -> Int32? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readI32()
    }

    func decodeIfPresent(_ type: Int64.Type, forKey key: Key) throws -> Int64? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readI64()
    }

    func decodeIfPresent(_ type: UInt.Type, forKey key: Key) throws -> UInt? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return UInt(try decoder.storage.readU64())
    }

    func decodeIfPresent(_ type: UInt8.Type, forKey key: Key) throws -> UInt8? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readU8()
    }

    func decodeIfPresent(_ type: UInt16.Type, forKey key: Key) throws -> UInt16? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readU16()
    }

    func decodeIfPresent(_ type: UInt32.Type, forKey key: Key) throws -> UInt32? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readU32()
    }

    func decodeIfPresent(_ type: UInt64.Type, forKey key: Key) throws -> UInt64? {
        let tag = try decoder.storage.readU8()
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.readU64()
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
        let tag = try decoder.storage.readU8()
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
        guard let tag = try? decoder.storage.readU8() else { return false }
        return tag == 1
    }
    
    func decode(_ type: Bool.Type) throws -> Bool { return try decoder.storage.readBool() }
    func decode(_ type: String.Type) throws -> String { return try decoder.storage.readString() }
    func decode(_ type: Double.Type) throws -> Double { return try decoder.storage.readDouble() }
    func decode(_ type: Float.Type) throws -> Float { return try decoder.storage.readFloat() }
    func decode(_ type: Int.Type) throws -> Int { return Int(try decoder.storage.readI64()) }
    func decode(_ type: Int8.Type) throws -> Int8 { return try decoder.storage.readI8() }
    func decode(_ type: Int16.Type) throws -> Int16 { return try decoder.storage.readI16() }
    func decode(_ type: Int32.Type) throws -> Int32 { return try decoder.storage.readI32() }
    func decode(_ type: Int64.Type) throws -> Int64 { return try decoder.storage.readI64() }
    func decode(_ type: UInt.Type) throws -> UInt { return UInt(try decoder.storage.readU64()) }
    func decode(_ type: UInt8.Type) throws -> UInt8 { return try decoder.storage.readU8() }
    func decode(_ type: UInt16.Type) throws -> UInt16 { return try decoder.storage.readU16() }
    func decode(_ type: UInt32.Type) throws -> UInt32 { return try decoder.storage.readU32() }
    func decode(_ type: UInt64.Type) throws -> UInt64 { return try decoder.storage.readU64() }
    
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
        let count = Int(try reader.readU32())
        if count == 0 {
            return []
        }

        if Element.self == UInt8.self {
            let values = try reader.readPrimitiveArray(UInt8.self, count: count)
            return values as! [Element]
        }
        if Element.self == Int32.self {
            let values = try reader.readPrimitiveArray(Int32.self, count: count)
            return values as! [Element]
        }
        if Element.self == UInt32.self {
            let values = try reader.readPrimitiveArray(UInt32.self, count: count)
            return values as! [Element]
        }
        if Element.self == Int64.self {
            let values = try reader.readPrimitiveArray(Int64.self, count: count)
            return values as! [Element]
        }
        if Element.self == UInt64.self {
            let values = try reader.readPrimitiveArray(UInt64.self, count: count)
            return values as! [Element]
        }
        if Element.self == Float.self {
            let values = try reader.readFloatArray(count: count)
            return values as! [Element]
        }
        if Element.self == Double.self {
            let values = try reader.readDoubleArray(count: count)
            return values as! [Element]
        }

        var decoded = Array<Element>()
        decoded.reserveCapacity(count)
        for _ in 0..<count {
            if let specialType = Element.self as? BSATNSpecialDecodable.Type {
                decoded.append(try specialType.decodeBSATN(from: reader) as! Element)
                continue
            }
            let decoder = _BSATNDecoder(storage: reader, codingPath: [])
            decoded.append(try Element(from: decoder))
        }
        return decoded
    }
}

extension Optional: BSATNSpecialDecodable where Wrapped: Decodable {
    public static func decodeBSATN(from reader: BSATNReader) throws -> Optional {
        let tag = try reader.readU8()
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
