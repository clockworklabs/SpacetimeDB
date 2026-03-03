import Compression
import Foundation

enum ServerMessageFrameDecodingError: Error {
    case emptyFrame
    case unsupportedCompression(UInt8)
    case invalidInputSize
    case initializationFailed
    case decompressionFailed
}

enum ServerMessageFrameDecoder {
    private static let compressionTagNone: UInt8 = 0
    private static let compressionTagBrotli: UInt8 = 1
    private static let compressionTagGzip: UInt8 = 2

    static func decodePayload(from frame: Data) throws -> Data {
        guard let compressionTag = frame.first else {
            throw ServerMessageFrameDecodingError.emptyFrame
        }

        let payload = Data(frame.dropFirst())
        switch compressionTag {
        case compressionTagNone:
            return payload
        case compressionTagBrotli:
            return try decompress(payload, algorithm: COMPRESSION_BROTLI)
        case compressionTagGzip:
            return try decompressGzip(payload)
        default:
            throw ServerMessageFrameDecodingError.unsupportedCompression(compressionTag)
        }
    }

    private static func decompress(_ payload: Data, algorithm: compression_algorithm) throws -> Data {
        if payload.isEmpty {
            return Data()
        }

        let destinationBufferSize = 64 * 1024
        let bootstrapPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: 1)
        defer { bootstrapPtr.deallocate() }
        var stream = compression_stream(
            dst_ptr: bootstrapPtr,
            dst_size: 0,
            src_ptr: UnsafePointer(bootstrapPtr),
            src_size: 0,
            state: nil
        )
        let initStatus = compression_stream_init(&stream, COMPRESSION_STREAM_DECODE, algorithm)
        guard initStatus != COMPRESSION_STATUS_ERROR else {
            throw ServerMessageFrameDecodingError.initializationFailed
        }
        defer { compression_stream_destroy(&stream) }

        return try payload.withUnsafeBytes { rawBuffer in
            guard let srcBase = rawBuffer.bindMemory(to: UInt8.self).baseAddress else {
                return Data()
            }

            stream.src_ptr = srcBase
            stream.src_size = payload.count

            let destinationBuffer = UnsafeMutablePointer<UInt8>.allocate(capacity: destinationBufferSize)
            defer { destinationBuffer.deallocate() }

            var output = Data()
            while true {
                stream.dst_ptr = destinationBuffer
                stream.dst_size = destinationBufferSize

                let status = compression_stream_process(&stream, Int32(COMPRESSION_STREAM_FINALIZE.rawValue))
                let produced = destinationBufferSize - stream.dst_size
                if produced > 0 {
                    output.append(destinationBuffer, count: produced)
                }

                switch status {
                case COMPRESSION_STATUS_OK:
                    continue
                case COMPRESSION_STATUS_END:
                    return output
                default:
                    throw ServerMessageFrameDecodingError.decompressionFailed
                }
            }
        }
    }

    private static func decompressGzip(_ payload: Data) throws -> Data {
        if payload.isEmpty {
            return Data()
        }

        guard payload.count >= 10 else {
            throw ServerMessageFrameDecodingError.invalidInputSize
        }

        var offset = 10
        let flags = payload[3]
        if flags & 0x04 != 0 {
            guard offset + 2 <= payload.count else { throw ServerMessageFrameDecodingError.invalidInputSize }
            let xlen = Int(payload[offset]) | (Int(payload[offset+1]) << 8)
            offset += 2 + xlen
        }
        if flags & 0x08 != 0 {
            while offset < payload.count && payload[offset] != 0 { offset += 1 }
            offset += 1
        }
        if flags & 0x10 != 0 {
            while offset < payload.count && payload[offset] != 0 { offset += 1 }
            offset += 1
        }
        if flags & 0x02 != 0 {
            offset += 2
        }
        
        guard offset <= payload.count - 8 else {
            throw ServerMessageFrameDecodingError.invalidInputSize
        }
        
        let deflateData = payload[offset ..< payload.count - 8]
        return try decompress(Data(deflateData), algorithm: COMPRESSION_ZLIB)
    }
}
