import Foundation

@_documentation(visibility: internal)
public enum BSATNDecodingError: Error, Equatable, BitwiseCopyable {
    case unexpectedEndOfData
    case invalidStringEncoding
    case invalidType
    case unsupportedType
}

@_documentation(visibility: internal)
public struct BSATNReader: ~Copyable {
    public let buffer: UnsafeRawBufferPointer
    public var offset: Int = 0
    
    public init(buffer: UnsafeRawBufferPointer, offset: Int = 0) {
        self.buffer = buffer
        self.offset = offset
    }
    
    public var isAtEnd: Bool {
        return offset >= buffer.count
    }
    
    public var remaining: Int {
        return max(0, buffer.count - offset)
    }
    
    public mutating func readBytes(count: Int) throws(BSATNDecodingError) -> Data {
        guard offset + count <= buffer.count else {
            throw .unexpectedEndOfData
        }
        let bytes = Data(buffer[offset..<(offset + count)])
        offset += count
        return bytes
    }
    
    public mutating func read<T: FixedWidthInteger>(_ type: T.Type) throws(BSATNDecodingError) -> T {
        let size = MemoryLayout<T>.size
        guard offset + size <= buffer.count else {
            throw .unexpectedEndOfData
        }
        let value = buffer.loadUnaligned(fromByteOffset: offset, as: T.self)
        offset += size
        return T(littleEndian: value)
    }

    public mutating func readU8() throws(BSATNDecodingError) -> UInt8 {
        guard offset + 1 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer[offset]
        offset += 1
        return val
    }

    public mutating func readU16() throws(BSATNDecodingError) -> UInt16 {
        guard offset + 2 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer.loadUnaligned(fromByteOffset: offset, as: UInt16.self)
        offset += 2
        return UInt16(littleEndian: val)
    }

    public mutating func readU32() throws(BSATNDecodingError) -> UInt32 {
        guard offset + 4 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer.loadUnaligned(fromByteOffset: offset, as: UInt32.self)
        offset += 4
        return UInt32(littleEndian: val)
    }

    public mutating func readU64() throws(BSATNDecodingError) -> UInt64 {
        guard offset + 8 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer.loadUnaligned(fromByteOffset: offset, as: UInt64.self)
        offset += 8
        return UInt64(littleEndian: val)
    }

    public mutating func readI8() throws(BSATNDecodingError) -> Int8 {
        guard offset + 1 <= buffer.count else { throw .unexpectedEndOfData }
        let val = Int8(bitPattern: buffer[offset])
        offset += 1
        return val
    }

    public mutating func readI16() throws(BSATNDecodingError) -> Int16 {
        guard offset + 2 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer.loadUnaligned(fromByteOffset: offset, as: Int16.self)
        offset += 2
        return Int16(littleEndian: val)
    }

    public mutating func readI32() throws(BSATNDecodingError) -> Int32 {
        guard offset + 4 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer.loadUnaligned(fromByteOffset: offset, as: Int32.self)
        offset += 4
        return Int32(littleEndian: val)
    }

    public mutating func readI64() throws(BSATNDecodingError) -> Int64 {
        guard offset + 8 <= buffer.count else { throw .unexpectedEndOfData }
        let val = buffer.loadUnaligned(fromByteOffset: offset, as: Int64.self)
        offset += 8
        return Int64(littleEndian: val)
    }
    
    public mutating func readDouble() throws(BSATNDecodingError) -> Double {
        guard offset + 8 <= buffer.count else {
            throw .unexpectedEndOfData
        }
        let bits = buffer.loadUnaligned(fromByteOffset: offset, as: UInt64.self)
        offset += 8
        return Double(bitPattern: UInt64(littleEndian: bits))
    }

    public mutating func readFloat() throws(BSATNDecodingError) -> Float {
        guard offset + 4 <= buffer.count else {
            throw .unexpectedEndOfData
        }
        let bits = buffer.loadUnaligned(fromByteOffset: offset, as: UInt32.self)
        offset += 4
        return Float(bitPattern: UInt32(littleEndian: bits))
    }

    public mutating func readString() throws -> String {
        let length = Int(try readU32())
        guard offset + length <= buffer.count else {
            throw BSATNDecodingError.unexpectedEndOfData
        }
        let stringStart = buffer.baseAddress!.advanced(by: offset).assumingMemoryBound(to: UInt8.self)
        let string = String(decoding: UnsafeBufferPointer(start: stringStart, count: length), as: UTF8.self)
        offset += length
        return string
    }

    public mutating func readBool() throws(BSATNDecodingError) -> Bool {
        let byte = try read(UInt8.self)
        switch byte {
        case 0: return false
        case 1: return true
        default: throw .invalidType
        }
    }

    public mutating func readArray<T>(_ block: (inout BSATNReader) throws -> T) throws -> [T] {
        let count = try readU32()
        var elements: [T] = []
        elements.reserveCapacity(Int(count))
        for _ in 0..<count {
            elements.append(try block(&self))
        }
        return elements
    }

    public mutating func readTaggedEnum<T>(_ block: (inout BSATNReader, UInt8) throws -> T) throws -> T {
        let tag = try readU8()
        return try block(&self, tag)
    }

    public mutating func readPrimitiveArray<T: FixedWidthInteger>(_ type: T.Type, count: Int) throws(BSATNDecodingError) -> [T] {
        let stride = MemoryLayout<T>.stride
        let byteCount = count * stride
        guard offset + byteCount <= buffer.count else {
            throw .unexpectedEndOfData
        }
        if count == 0 { return [] }

        let src = buffer.baseAddress!.advanced(by: offset)
        let values: [T] = Array(unsafeUninitializedCapacity: count) { bufferOut, initializedCount in
            memcpy(bufferOut.baseAddress!, src, byteCount)
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

    public mutating func readFloatArray(count: Int) throws(BSATNDecodingError) -> [Float] {
        let bits = try readPrimitiveArray(UInt32.self, count: count)
        var values = Array<Float>()
        values.reserveCapacity(bits.count)
        for bitPattern in bits {
            values.append(Float(bitPattern: bitPattern))
        }
        return values
    }


    public mutating func readDoubleArray(count: Int) throws(BSATNDecodingError) -> [Double] {
        let bits = try readPrimitiveArray(UInt64.self, count: count)
        var values = Array<Double>()
        values.reserveCapacity(bits.count)
        for bitPattern in bits {
            values.append(Double(bitPattern: bitPattern))
        }
        return values
    }

    public mutating func fallbackDecode<T: Decodable>(_ type: T.Type) throws -> T {
        let wrapper = BSATNReaderWrapper(reader: BSATNReader(buffer: buffer, offset: offset))
        let decoder = _BSATNDecoder(storage: wrapper, codingPath: [])
        let result = try T(from: decoder)
        self.offset = wrapper.reader.offset
        return result
    }
}

@_documentation(visibility: internal)
public protocol BSATNFastCopyable: BitwiseCopyable {}
extension UInt8: BSATNFastCopyable {}
extension Int8: BSATNFastCopyable {}
extension UInt16: BSATNFastCopyable {}
extension Int16: BSATNFastCopyable {}
extension UInt32: BSATNFastCopyable {}
extension Int32: BSATNFastCopyable {}
extension UInt64: BSATNFastCopyable {}
extension Int64: BSATNFastCopyable {}
extension Float: BSATNFastCopyable {}
extension Double: BSATNFastCopyable {}

class BSATNReaderWrapper {
    var reader: BSATNReader
    init(reader: consuming BSATNReader) {
        self.reader = reader
    }
    
    func withReader<T>(_ block: (inout BSATNReader) throws -> T) rethrows -> T {
        try block(&reader)
    }
}

@_documentation(visibility: internal)
public final class BSATNDecoder: Sendable {
    public init() {}
    
    public func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        return try data.withUnsafeBytes { buffer in
            var reader = BSATNReader(buffer: buffer)
            if let specialType = type as? BSATNSpecialDecodable.Type {
                return try specialType.decodeBSATN(from: &reader) as! T
            }
            let wrapper = BSATNReaderWrapper(reader: BSATNReader(buffer: reader.buffer, offset: reader.offset))
            let decoder = _BSATNDecoder(storage: wrapper, codingPath: [])
            return try T(from: decoder)
        }
    }
    
    public func decode(_ type: String.Type, from data: Data) throws -> String {
        return try data.withUnsafeBytes { buffer in
            var reader = BSATNReader(buffer: buffer)
            return try reader.readString()
        }
    }
}

struct _BSATNDecoder: Decoder {
    var storage: BSATNReaderWrapper
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
        let tag = try decoder.storage.withReader { try $0.readU8() }
        switch tag {
        case 0: return false
        case 1: return true
        default: throw BSATNDecodingError.invalidType
        }
    }
    
    func decode<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T {
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try decoder.storage.withReader { try specialType.decodeBSATN(from: &$0) } as! T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent<T: Decodable>(_ type: T.Type, forKey key: Key) throws -> T? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try decoder.storage.withReader { try specialType.decodeBSATN(from: &$0) } as? T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath + [key])
        return try T(from: childDecoder)
    }
    
    func decodeIfPresent(_ type: Bool.Type, forKey key: Key) throws -> Bool? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readBool() }
    }

    func decodeIfPresent(_ type: String.Type, forKey key: Key) throws -> String? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readString() }
    }

    func decodeIfPresent(_ type: Float.Type, forKey key: Key) throws -> Float? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readFloat() }
    }

    func decodeIfPresent(_ type: Double.Type, forKey key: Key) throws -> Double? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readDouble() }
    }

    func decodeIfPresent(_ type: Int.Type, forKey key: Key) throws -> Int? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return Int(try decoder.storage.withReader { try $0.readI64() })
    }

    func decodeIfPresent(_ type: Int8.Type, forKey key: Key) throws -> Int8? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readI8() }
    }

    func decodeIfPresent(_ type: Int16.Type, forKey key: Key) throws -> Int16? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readI16() }
    }

    func decodeIfPresent(_ type: Int32.Type, forKey key: Key) throws -> Int32? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readI32() }
    }

    func decodeIfPresent(_ type: Int64.Type, forKey key: Key) throws -> Int64? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readI64() }
    }

    func decodeIfPresent(_ type: UInt.Type, forKey key: Key) throws -> UInt? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return UInt(try decoder.storage.withReader { try $0.readU64() })
    }

    func decodeIfPresent(_ type: UInt8.Type, forKey key: Key) throws -> UInt8? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readU8() }
    }

    func decodeIfPresent(_ type: UInt16.Type, forKey key: Key) throws -> UInt16? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readU16() }
    }

    func decodeIfPresent(_ type: UInt32.Type, forKey key: Key) throws -> UInt32? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readU32() }
    }

    func decodeIfPresent(_ type: UInt64.Type, forKey key: Key) throws -> UInt64? {
        let tag = try decoder.storage.withReader { try $0.readU8() }
        if tag == 1 { return nil }
        guard tag == 0 else { throw BSATNDecodingError.invalidType }
        return try decoder.storage.withReader { try $0.readU64() }
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
        return decoder.storage.withReader { $0.isAtEnd }
    }
    var currentIndex: Int = 0
    
    mutating func decodeNil() throws -> Bool {
        let tag = try decoder.storage.withReader { try $0.readU8() }
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
            value = try decoder.storage.withReader { try specialType.decodeBSATN(from: &$0) } as! T
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
        guard let tag = try? decoder.storage.reader.readU8() else { return false }
        return tag == 1
    }
    
    func decode(_ type: Bool.Type) throws -> Bool { return try decoder.storage.withReader { try $0.readBool() } }
    func decode(_ type: String.Type) throws -> String { return try decoder.storage.withReader { try $0.readString() } }
    func decode(_ type: Double.Type) throws -> Double { return try decoder.storage.withReader { try $0.readDouble() } }
    func decode(_ type: Float.Type) throws -> Float { return try decoder.storage.withReader { try $0.readFloat() } }
    func decode(_ type: Int.Type) throws -> Int { return Int(try decoder.storage.withReader { try $0.readI64() }) }
    func decode(_ type: Int8.Type) throws -> Int8 { return try decoder.storage.withReader { try $0.readI8() } }
    func decode(_ type: Int16.Type) throws -> Int16 { return try decoder.storage.withReader { try $0.readI16() } }
    func decode(_ type: Int32.Type) throws -> Int32 { return try decoder.storage.withReader { try $0.readI32() } }
    func decode(_ type: Int64.Type) throws -> Int64 { return try decoder.storage.withReader { try $0.readI64() } }
    func decode(_ type: UInt.Type) throws -> UInt { return UInt(try decoder.storage.withReader { try $0.readU64() }) }
    func decode(_ type: UInt8.Type) throws -> UInt8 { return try decoder.storage.withReader { try $0.readU8() } }
    func decode(_ type: UInt16.Type) throws -> UInt16 { return try decoder.storage.withReader { try $0.readU16() } }
    func decode(_ type: UInt32.Type) throws -> UInt32 { return try decoder.storage.withReader { try $0.readU32() } }
    func decode(_ type: UInt64.Type) throws -> UInt64 { return try decoder.storage.withReader { try $0.readU64() } }
    
    func decode<T: Decodable>(_ type: T.Type) throws -> T {
        if let specialType = type as? BSATNSpecialDecodable.Type {
            return try decoder.storage.withReader { try specialType.decodeBSATN(from: &$0) } as! T
        }
        let childDecoder = _BSATNDecoder(storage: decoder.storage, codingPath: codingPath)
        return try T(from: childDecoder)
    }
}

@_documentation(visibility: internal)
public protocol BSATNSpecialDecodable {
    static func decodeBSATN(from reader: inout BSATNReader) throws -> Self
}

extension Array: BSATNSpecialDecodable where Element: Decodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> Array {
        let count = Int(try reader.readU32())
        if count == 0 {
            return []
        }

        #if _endian(little)
        if Element.self is any BSATNFastCopyable.Type {
            let stride = MemoryLayout<Element>.stride
            let byteCount = count * stride
            guard reader.offset + byteCount <= reader.buffer.count else {
                throw BSATNDecodingError.unexpectedEndOfData
            }
            let src = reader.buffer.baseAddress!.advanced(by: reader.offset)
            let values: [Element] = Array(unsafeUninitializedCapacity: count) { bufferOut, initializedCount in
                memcpy(bufferOut.baseAddress!, src, byteCount)
                initializedCount = count
            }
            reader.offset += byteCount
            return values
        }
        #endif

        var decoded = Array<Element>()
        decoded.reserveCapacity(count)
        for _ in 0..<count {
            if let specialType = Element.self as? BSATNSpecialDecodable.Type {
                decoded.append(try specialType.decodeBSATN(from: &reader) as! Element)
                continue
            }
            let wrapper = BSATNReaderWrapper(reader: BSATNReader(buffer: reader.buffer, offset: reader.offset))
            let decoder = _BSATNDecoder(storage: wrapper, codingPath: [])
            decoded.append(try Element(from: decoder))
            reader.offset = wrapper.reader.offset
        }
        return decoded
    }
}

extension Optional: BSATNSpecialDecodable where Wrapped: Decodable {
    public static func decodeBSATN(from reader: inout BSATNReader) throws -> Optional {
        let tag = try reader.readU8()
        if tag == 1 {
            return .none
        } else if tag == 0 {
            if let specialType = Wrapped.self as? BSATNSpecialDecodable.Type {
                return .some(try specialType.decodeBSATN(from: &reader) as! Wrapped)
            }
            let wrapper = BSATNReaderWrapper(reader: BSATNReader(buffer: reader.buffer, offset: reader.offset))
            let decoder = _BSATNDecoder(storage: wrapper, codingPath: [])
            let result: Optional = .some(try Wrapped(from: decoder))
            reader.offset = wrapper.reader.offset
            return result
        } else {
            throw BSATNDecodingError.invalidType
        }
    }
}
