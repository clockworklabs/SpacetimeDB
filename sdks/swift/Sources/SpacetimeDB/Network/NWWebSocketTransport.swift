import Foundation
import Network
import Synchronization
import CryptoKit

protocol WebSocketTransportDelegate: AnyObject, Sendable {
    func webSocketTransportDidConnect()
    func webSocketTransportDidDisconnect(error: Error?)
    func webSocketTransportDidReceive(data: Data)
    func webSocketTransportDidReceivePong()
}

protocol WebSocketTransport: AnyObject, Sendable {
    var delegate: WebSocketTransportDelegate? { get set }
    func connect(to url: URL, protocols: [String], headers: [String: String])
    func disconnect()
    func send(data: Data, completion: @escaping @Sendable (Error?) -> Void)
    func sendPing(completion: @escaping @Sendable (Error?) -> Void)
}

final class NWWebSocketTransport: WebSocketTransport, @unchecked Sendable {
    private let stateLock: Mutex<Void> = Mutex(())
    private var connection: NWConnection?
    private weak var _delegate: WebSocketTransportDelegate?
    private var closingConnection = false
    private var didCompleteHandshake = false
    private var handshakeKey = ""
    private var incomingBuffer = Data()
    private let queue = DispatchQueue(label: "spacetimedb.transport.nw", qos: .userInitiated)

    var delegate: WebSocketTransportDelegate? {
        get { stateLock.withLock { _ in _delegate } }
        set { stateLock.withLock { _ in _delegate = newValue } }
    }

    func connect(to url: URL, protocols: [String], headers: [String: String]) {
        guard let host = url.host else {
            delegate?.webSocketTransportDidDisconnect(error: NSError(domain: "NWWebSocketTransport", code: 0, userInfo: [NSLocalizedDescriptionKey: "Missing host in URL"]))
            return
        }
        let isSecure = (url.scheme == "wss" || url.scheme == "https")
        let port = NWEndpoint.Port(integerLiteral: NWEndpoint.Port.IntegerLiteralType(url.port ?? (isSecure ? 443 : 80)))
        let tcp = NWProtocolTCP.Options()
        let parameters = NWParameters(tls: isSecure ? NWProtocolTLS.Options() : nil, tcp: tcp)
        let connection = NWConnection(host: NWEndpoint.Host(host), port: port, using: parameters)

        self.stateLock.withLock { _ in
            self.closingConnection = false
            self.didCompleteHandshake = false
            self.incomingBuffer.removeAll(keepingCapacity: true)
            self.handshakeKey = Self.makeSecWebSocketKey()
            self.connection?.cancel()
            self.connection = connection
        }

        connection.stateUpdateHandler = { [weak self] state in
            self?.handleStateChange(
                state,
                url: url,
                protocols: protocols,
                headers: headers
            )
        }

        connection.start(queue: queue)
    }

    private func handleStateChange(
        _ state: NWConnection.State,
        url: URL,
        protocols: [String],
        headers: [String: String]
    ) {
        switch state {
        case .ready:
            sendHandshake(url: url, protocols: protocols, headers: headers)
        case .failed(let error):
            stateLock.withLock { _ in connection = nil }
            delegate?.webSocketTransportDidDisconnect(error: error)
        case .cancelled:
            let shouldNotify = stateLock.withLock { _ in
                let notify = !closingConnection
                connection = nil
                closingConnection = false
                return notify
            }
            if shouldNotify {
                delegate?.webSocketTransportDidDisconnect(error: nil)
            }
        case .waiting(let error):
            Log.network.debug("NWConnection waiting: \(error.localizedDescription)")
        case .preparing, .setup:
            break
        @unknown default:
            break
        }
    }

    private func sendHandshake(url: URL, protocols: [String], headers: [String: String]) {
        guard let connection = stateLock.withLock({ _ in self.connection }) else {
            delegate?.webSocketTransportDidDisconnect(error: NSError(domain: "NWWebSocketTransport", code: 0, userInfo: [NSLocalizedDescriptionKey: "Connection not available"]))
            return
        }
        let key = stateLock.withLock { _ in handshakeKey }
        let request = makeHandshakeRequest(url: url, key: key, protocols: protocols, headers: headers)
        connection.send(content: request, completion: .contentProcessed { [weak self] error in
            guard let self else { return }
            if let error {
                self.delegate?.webSocketTransportDidDisconnect(error: error)
                return
            }
            self.receiveHandshakeResponse()
        })
    }

    private func makeHandshakeRequest(url: URL, key: String, protocols: [String], headers: [String: String]) -> Data {
        let path = (url.path.isEmpty ? "/" : url.path) + (url.query.map { "?\($0)" } ?? "")
        let isSecure = (url.scheme == "wss" || url.scheme == "https")
        let defaultPort = isSecure ? 443 : 80
        let host = url.host ?? "localhost"
        let hostHeader: String
        if let port = url.port, port != defaultPort {
            hostHeader = "\(host):\(port)"
        } else {
            hostHeader = host
        }

        var lines = [
            "GET \(path) HTTP/1.1",
            "Host: \(hostHeader)",
            "Upgrade: websocket",
            "Connection: Upgrade",
            "Sec-WebSocket-Version: 13",
            "Sec-WebSocket-Key: \(key)"
        ]
        if !protocols.isEmpty {
            lines.append("Sec-WebSocket-Protocol: \(protocols.joined(separator: ", "))")
        }
        for (name, value) in headers where name.caseInsensitiveCompare("Host") != .orderedSame {
            lines.append("\(name): \(value)")
        }
        let request = lines.joined(separator: "\r\n") + "\r\n\r\n"
        return Data(request.utf8)
    }

    private func receiveHandshakeResponse() {
        guard let connection = stateLock.withLock({ _ in self.connection }) else { return }
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] content, _, isComplete, error in
            guard let self else { return }
            if let error {
                self.delegate?.webSocketTransportDidDisconnect(error: error)
                return
            }
            if isComplete, content == nil {
                self.delegate?.webSocketTransportDidDisconnect(error: nil)
                return
            }

            if let content {
                let done = self.stateLock.withLock { _ -> Bool in
                    self.incomingBuffer.append(content)
                    guard let range = self.incomingBuffer.range(of: Data("\r\n\r\n".utf8)) else {
                        return false
                    }
                    let headerData = self.incomingBuffer[..<range.upperBound]
                    let remaining = self.incomingBuffer[range.upperBound...]
                    self.incomingBuffer = Data(remaining)
                    return self.validateHandshakeResponse(headerData)
                }
                if done {
                    self.stateLock.withLock { _ in self.didCompleteHandshake = true }
                    self.delegate?.webSocketTransportDidConnect()
                    self.receiveNextMessage()
                    return
                }
            }
            self.receiveHandshakeResponse()
        }
    }

    private func validateHandshakeResponse(_ headerData: Data.SubSequence) -> Bool {
        guard let headerString = String(data: Data(headerData), encoding: .utf8) else { return false }
        let lines = headerString.split(separator: "\r\n", omittingEmptySubsequences: false).map(String.init)
        guard let statusLine = lines.first, statusLine.contains(" 101 ") else { return false }
        var acceptValue: String?
        for line in lines.dropFirst() {
            guard let sep = line.firstIndex(of: ":") else { continue }
            let name = line[..<sep].trimmingCharacters(in: .whitespaces)
            let value = line[line.index(after: sep)...].trimmingCharacters(in: .whitespaces)
            if name.caseInsensitiveCompare("Sec-WebSocket-Accept") == .orderedSame {
                acceptValue = value
            }
        }
        guard let acceptValue else { return false }
        let expected = Self.computeAcceptKey(from: handshakeKey)
        return acceptValue == expected
    }

    func disconnect() {
        stateLock.withLock { _ in
            closingConnection = true
            connection?.cancel()
            connection = nil
            incomingBuffer.removeAll(keepingCapacity: true)
            didCompleteHandshake = false
        }
    }

    func send(data: Data, completion: @escaping @Sendable (Error?) -> Void) {
        let connection = stateLock.withLock { _ in self.connection }
        guard let connection = connection else {
            completion(NSError(domain: "NWWebSocketTransport", code: 0, userInfo: [NSLocalizedDescriptionKey: "Not connected"]))
            return
        }

        let frame = Self.makeFrame(opcode: 0x2, payload: data)
        connection.send(content: frame, completion: .contentProcessed { error in
            completion(error)
        })
    }

    func sendPing(completion: @escaping @Sendable (Error?) -> Void) {
        let connection = stateLock.withLock { _ in self.connection }
        guard let connection = connection else {
            completion(NSError(domain: "NWWebSocketTransport", code: 0, userInfo: [NSLocalizedDescriptionKey: "Not connected"]))
            return
        }

        let frame = Self.makeFrame(opcode: 0x9, payload: Data())
        connection.send(content: frame, completion: .contentProcessed { error in
            completion(error)
        })
    }

    private func receiveNextMessage() {
        let connection = stateLock.withLock { _ in self.connection }
        guard let connection = connection else { return }

        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] content, _, isComplete, error in
            guard let self = self else { return }

            if let error {
                self.delegate?.webSocketTransportDidDisconnect(error: error)
                return
            }
            if isComplete, content == nil {
                self.delegate?.webSocketTransportDidDisconnect(error: nil)
                return
            }

            if let content, !content.isEmpty {
                var frames: [(UInt8, Data)] = []
                self.stateLock.withLock { _ in
                    self.incomingBuffer.append(content)
                    frames = Self.parseFrames(from: &self.incomingBuffer)
                }
                for (opcode, payload) in frames {
                    switch opcode {
                    case 0x2:
                        self.delegate?.webSocketTransportDidReceive(data: payload)
                    case 0x9:
                        let pong = Self.makeFrame(opcode: 0xA, payload: payload)
                        connection.send(content: pong, completion: .contentProcessed { _ in })
                    case 0xA:
                        self.delegate?.webSocketTransportDidReceivePong()
                    case 0x8:
                        self.disconnect()
                    default:
                        break
                    }
                }
            }

            self.receiveNextMessage()
        }
    }

    private static func makeSecWebSocketKey() -> String {
        var bytes = [UInt8](repeating: 0, count: 16)
        for i in bytes.indices {
            bytes[i] = UInt8.random(in: 0...255)
        }
        return Data(bytes).base64EncodedString()
    }

    private static func computeAcceptKey(from key: String) -> String {
        let magic = key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
        let digest = Insecure.SHA1.hash(data: Data(magic.utf8))
        return Data(digest).base64EncodedString()
    }

    private static func makeFrame(opcode: UInt8, payload: Data) -> Data {
        var frame = Data()
        frame.append(0x80 | (opcode & 0x0F))
        let maskBit: UInt8 = 0x80
        let length = payload.count
        if length <= 125 {
            frame.append(maskBit | UInt8(length))
        } else if length <= 0xFFFF {
            frame.append(maskBit | 126)
            frame.append(UInt8((length >> 8) & 0xFF))
            frame.append(UInt8(length & 0xFF))
        } else {
            frame.append(maskBit | 127)
            let l = UInt64(length)
            for shift in stride(from: 56, through: 0, by: -8) {
                frame.append(UInt8((l >> UInt64(shift)) & 0xFF))
            }
        }
        let mask: [UInt8] = (0..<4).map { _ in UInt8.random(in: 0...255) }
        frame.append(contentsOf: mask)
        for (index, byte) in payload.enumerated() {
            frame.append(byte ^ mask[index % 4])
        }
        return frame
    }

    private static func parseFrames(from buffer: inout Data) -> [(UInt8, Data)] {
        var frames: [(UInt8, Data)] = []
        var index = 0
        while true {
            let start = index
            guard buffer.count - index >= 2 else { break }
            let first = buffer[index]
            let second = buffer[index + 1]
            index += 2

            let opcode = first & 0x0F
            let masked = (second & 0x80) != 0
            var payloadLength = Int(second & 0x7F)
            if payloadLength == 126 {
                guard buffer.count - index >= 2 else { index = start; break }
                payloadLength = Int(buffer[index]) << 8 | Int(buffer[index + 1])
                index += 2
            } else if payloadLength == 127 {
                guard buffer.count - index >= 8 else { index = start; break }
                var len: UInt64 = 0
                for b in buffer[index..<(index + 8)] {
                    len = (len << 8) | UInt64(b)
                }
                guard len <= UInt64(Int.max) else { index = start; break }
                payloadLength = Int(len)
                index += 8
            }

            var maskingKey: [UInt8] = [0, 0, 0, 0]
            if masked {
                guard buffer.count - index >= 4 else { index = start; break }
                maskingKey = Array(buffer[index..<(index + 4)])
                index += 4
            }
            guard buffer.count - index >= payloadLength else { index = start; break }

            var payload = Data(buffer[index..<(index + payloadLength)])
            index += payloadLength
            if masked {
                let payloadCount = payload.count
                payload.withUnsafeMutableBytes { bytes in
                    guard let ptr = bytes.bindMemory(to: UInt8.self).baseAddress else { return }
                    for i in 0..<payloadCount {
                        ptr[i] ^= maskingKey[i % 4]
                    }
                }
            }
            frames.append((opcode, payload))
        }
        if index > 0 {
            buffer.removeSubrange(0..<index)
        }
        return frames
    }
}
