import XCTest
@testable import SpacetimeDB

final class ProtocolParityTests: XCTestCase {
    
    private let encoder = BSATNEncoder()
    private let decoder = BSATNDecoder()
    
    func testSubscribeEncodingParity() throws {
        let msg = ClientMessage.subscribe(Subscribe(
            queryStrings: ["SELECT * FROM player"],
            requestId: RequestId(rawValue: 1),
            querySetId: QuerySetId(rawValue: 1)
        ))
        
        let encoded = try encoder.encode(msg)
        let hex = encoded.map { String(format: "%02x", $0) }.joined()
        
        // Tag 0 (Subscribe)
        // RequestId 1: 01000000
        // QuerySetId 1: 01000000
        // Array Len 1: 01000000
        // String Len 20: 14000000
        // String "SELECT * FROM player"
        let expectedHex = "000100000001000000010000001400000053454c454354202a2046524f4d20706c61796572"
        XCTAssertEqual(hex, expectedHex)
    }
    
    func testCallReducerEncodingParity() throws {
        let args = Data([0xDE, 0xAD, 0xBE, 0xEF])
        let msg = ClientMessage.callReducer(CallReducer(
            requestId: RequestId(rawValue: 42),
            flags: 0,
            reducer: "move",
            args: args
        ))
        
        let encoded = try encoder.encode(msg)
        let hex = encoded.map { String(format: "%02x", $0) }.joined()
        
        // Tag 3 (CallReducer)
        // RequestId 42: 2a000000
        // Flags 0: 00
        // Reducer name "move" (len 4: 04000000, bytes: 6d6f7665)
        // Args len 4: 04000000, bytes: deadbeef
        let expectedHex = "032a00000000040000006d6f766504000000deadbeef"
        XCTAssertEqual(hex, expectedHex)
    }
    
    func testInitialConnectionDecodingParity() throws {
        // Tag 0 (InitialConnection)
        // Identity (32 bytes zeros)
        // ConnectionId (16 bytes, let's say 1...)
        // Token len 5: 05000000, "hello"
        var hex = "00" // tag
        hex += String(repeating: "00", count: 32) // identity
        hex += "01000000000000000000000000000000" // connection id
        hex += "0500000068656c6c6f" // token
        
        let data = Data(hexString: hex)
        let msg = try decoder.decode(ServerMessage.self, from: data)
        
        if case .initialConnection(let conn) = msg {
            XCTAssertEqual(conn.token, "hello")
            XCTAssertEqual(conn.identity.rawBytes.count, 32)
            XCTAssertEqual(conn.connectionId.rawBytes[0], 1)
        } else {
            XCTFail("Wrong message type: \(msg)")
        }
    }
}

extension Data {
    init(hexString: String) {
        var data = Data()
        var hex = hexString
        while hex.count > 0 {
            let subIndex = hex.index(hex.startIndex, offsetBy: 2)
            let c = String(hex[..<subIndex])
            hex = String(hex[subIndex...])
            if let b = UInt8(c, radix: 16) {
                data.append(b)
            }
        }
        self = data
    }
}
