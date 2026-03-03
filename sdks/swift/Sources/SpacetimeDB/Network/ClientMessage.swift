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
    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        switch self {
        case .subscribe(let msg):
            storage.append(0 as UInt8)
            try msg.encodeBSATN(to: &storage)
        case .unsubscribe(let msg):
            storage.append(1 as UInt8)
            try msg.encodeBSATN(to: &storage)
        case .oneOffQuery(let msg):
            storage.append(2 as UInt8)
            try msg.encodeBSATN(to: &storage)
        case .callReducer(let msg):
            storage.append(3 as UInt8)
            try msg.encodeBSATN(to: &storage)
        case .callProcedure(let msg):
            storage.append(4 as UInt8)
            try msg.encodeBSATN(to: &storage)
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

    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        try requestId.encodeBSATN(to: &storage)
        try querySetId.encodeBSATN(to: &storage)
        storage.append(UInt32(queryStrings.count))
        for query in queryStrings {
            try storage.appendString(query)
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

    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        try requestId.encodeBSATN(to: &storage)
        try querySetId.encodeBSATN(to: &storage)
        storage.append(flags)
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

    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        try requestId.encodeBSATN(to: &storage)
        try storage.appendString(queryString)
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

    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        try requestId.encodeBSATN(to: &storage)
        storage.append(flags)
        try storage.appendString(reducer)
        storage.append(UInt32(args.count))
        storage.append(args)
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

    public func encodeBSATN(to storage: inout BSATNStorage) throws {
        try requestId.encodeBSATN(to: &storage)
        storage.append(flags)
        try storage.appendString(procedure)
        storage.append(UInt32(args.count))
        storage.append(args)
    }
}
