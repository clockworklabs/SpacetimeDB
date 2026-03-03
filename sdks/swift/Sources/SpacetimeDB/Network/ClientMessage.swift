import Foundation

// MARK: - Client Messages (Encoding)

/// Tag order for Rust `websocket::v2::ClientMessage`:
/// 0 = Subscribe
/// 1 = Unsubscribe
/// 2 = OneOffQuery
/// 3 = CallReducer
/// 4 = CallProcedure
public enum ClientMessage: Encodable {
    case subscribe(Subscribe)
    case unsubscribe(Unsubscribe)
    case oneOffQuery(OneOffQuery)
    case callReducer(CallReducer)
    case callProcedure(CallProcedure)

    public func encode(to encoder: Encoder) throws {}
}

extension ClientMessage: BSATNSpecialEncodable {
    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        switch self {
        case .subscribe(let msg):
            encoder.storage.append(0 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .unsubscribe(let msg):
            encoder.storage.append(1 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .oneOffQuery(let msg):
            encoder.storage.append(2 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .callReducer(let msg):
            encoder.storage.append(3 as UInt8)
            try msg.encodeBSATN(to: encoder)
        case .callProcedure(let msg):
            encoder.storage.append(4 as UInt8)
            try msg.encodeBSATN(to: encoder)
        }
    }
}

/// Rust: `Subscribe { request_id: u32, query_set_id: QuerySetId, query_strings: Box<[Box<str>]> }`
public struct Subscribe: Encodable, BSATNSpecialEncodable {
    public var requestId: RequestId
    public var querySetId: QuerySetId
    public var queryStrings: [String]

    public init(queryStrings: [String], requestId: RequestId, querySetId: QuerySetId = QuerySetId(rawValue: 1)) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.queryStrings = queryStrings
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        try requestId.encodeBSATN(to: encoder)
        try querySetId.encodeBSATN(to: encoder)
        encoder.storage.append(UInt32(queryStrings.count))
        var singleVal = encoder.singleValueContainer()
        for query in queryStrings {
            try singleVal.encode(query)
        }
    }
}

/// Rust: `Unsubscribe { request_id: u32, query_set_id: QuerySetId, flags: u8 }`
public struct Unsubscribe: Encodable, BSATNSpecialEncodable {
    public var requestId: RequestId
    public var querySetId: QuerySetId
    public var flags: UInt8

    public init(requestId: RequestId, querySetId: QuerySetId, flags: UInt8 = 0) {
        self.requestId = requestId
        self.querySetId = querySetId
        self.flags = flags
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        try requestId.encodeBSATN(to: encoder)
        try querySetId.encodeBSATN(to: encoder)
        encoder.storage.append(flags)
    }
}

/// Rust: `OneOffQuery { request_id: u32, query_string: Box<str> }`
public struct OneOffQuery: Encodable, BSATNSpecialEncodable {
    public var requestId: RequestId
    public var queryString: String

    public init(requestId: RequestId, queryString: String) {
        self.requestId = requestId
        self.queryString = queryString
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        try requestId.encodeBSATN(to: encoder)
        var singleVal = encoder.singleValueContainer()
        try singleVal.encode(queryString)
    }
}

/// Rust: `CallReducer { request_id: u32, flags: u8, reducer: Box<str>, args: Bytes }`
public struct CallReducer: Encodable, BSATNSpecialEncodable {
    public var requestId: RequestId
    public var flags: UInt8
    public var reducer: String
    public var args: Data

    public init(requestId: RequestId, flags: UInt8, reducer: String, args: Data) {
        self.requestId = requestId
        self.flags = flags
        self.reducer = reducer
        self.args = args
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        try requestId.encodeBSATN(to: encoder)
        encoder.storage.append(flags)

        var singleVal = encoder.singleValueContainer()
        try singleVal.encode(reducer)

        encoder.storage.append(UInt32(args.count))
        encoder.storage.append(args)
    }
}

/// Rust: `CallProcedure { request_id: u32, flags: u8, procedure: Box<str>, args: Bytes }`
public struct CallProcedure: Encodable, BSATNSpecialEncodable {
    public var requestId: RequestId
    public var flags: UInt8
    public var procedure: String
    public var args: Data

    public init(requestId: RequestId, flags: UInt8, procedure: String, args: Data) {
        self.requestId = requestId
        self.flags = flags
        self.procedure = procedure
        self.args = args
    }

    public func encode(to encoder: Encoder) throws {}

    func encodeBSATN(to encoder: _BSATNEncoder) throws {
        try requestId.encodeBSATN(to: encoder)
        encoder.storage.append(flags)

        var singleVal = encoder.singleValueContainer()
        try singleVal.encode(procedure)

        encoder.storage.append(UInt32(args.count))
        encoder.storage.append(args)
    }
}
