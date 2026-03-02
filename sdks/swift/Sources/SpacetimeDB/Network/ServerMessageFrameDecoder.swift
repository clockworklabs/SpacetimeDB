import Compression
import Foundation
import zlib

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

        guard payload.count <= Int(UInt32.max) else {
            throw ServerMessageFrameDecodingError.invalidInputSize
        }

        return try payload.withUnsafeBytes { rawBuffer in
            guard let srcBase = rawBuffer.bindMemory(to: Bytef.self).baseAddress else {
                return Data()
            }

            var stream = z_stream()
            stream.next_in = UnsafeMutablePointer<Bytef>(mutating: srcBase)
            stream.avail_in = uInt(payload.count)

            let initStatus = inflateInit2_(&stream, 47, ZLIB_VERSION, Int32(MemoryLayout<z_stream>.size))
            guard initStatus == Z_OK else {
                throw ServerMessageFrameDecodingError.initializationFailed
            }
            defer { inflateEnd(&stream) }

            let destinationBufferSize = 64 * 1024
            var destinationBuffer = [UInt8](repeating: 0, count: destinationBufferSize)
            var output = Data()

            while true {
                let inflateStatus: Int32 = destinationBuffer.withUnsafeMutableBytes { outRaw in
                    stream.next_out = outRaw.bindMemory(to: Bytef.self).baseAddress
                    stream.avail_out = uInt(destinationBufferSize)
                    return inflate(&stream, Z_NO_FLUSH)
                }

                let produced = destinationBufferSize - Int(stream.avail_out)
                if produced > 0 {
                    output.append(contentsOf: destinationBuffer[0..<produced])
                }

                switch inflateStatus {
                case Z_OK:
                    continue
                case Z_STREAM_END:
                    return output
                default:
                    throw ServerMessageFrameDecodingError.decompressionFailed
                }
            }
        }
    }
}
