import Foundation
import Synchronization

public enum SpacetimeClientProcedureError: Error, Equatable {
    case internalError(String)
    case disconnected
    case timeout
}

public enum SpacetimeClientConnectionError: Error, Equatable {
    case keepAliveTimeout
}

private final class AsyncResponseContinuation<Value: Sendable>: @unchecked Sendable {
    private let stateLock: Mutex<Void> = Mutex(())
    private var continuation: CheckedContinuation<Value, Error>?
    private var timeoutTask: Task<Void, Never>?
    private var completionResult: Result<Value, Error>?
    private var isCompleted = false

    @inline(__always)
    private func withStateLock<R>(_ body: () throws -> R) rethrows -> R {
        try stateLock.withLock { _ in
            try body()
        }
    }

    func install(_ continuation: CheckedContinuation<Value, Error>) {
        var resultToResume: Result<Value, Error>?

        withStateLock {
            if isCompleted {
                resultToResume = completionResult
                completionResult = nil
            } else {
                self.continuation = continuation
            }
        }

        if let resultToResume {
            resume(continuation, with: resultToResume)
        }
    }

    func setTimeoutTask(_ task: Task<Void, Never>) {
        var shouldCancelTask = false

        withStateLock {
            if isCompleted {
                shouldCancelTask = true
            } else {
                timeoutTask = task
            }
        }

        if shouldCancelTask {
            task.cancel()
        }
    }

    func resolve(_ result: Result<Value, Error>) {
        var continuationToResume: CheckedContinuation<Value, Error>?
        var timeoutTaskToCancel: Task<Void, Never>?

        let didResolve = withStateLock { () -> Bool in
            if isCompleted {
                return false
            }
            isCompleted = true
            timeoutTaskToCancel = timeoutTask
            timeoutTask = nil

            if let continuation {
                continuationToResume = continuation
                self.continuation = nil
            } else {
                completionResult = result
            }
            return true
        }
        guard didResolve else { return }

        timeoutTaskToCancel?.cancel()
        if let continuationToResume {
            resume(continuationToResume, with: result)
        }
    }

    private func resume(_ continuation: CheckedContinuation<Value, Error>, with result: Result<Value, Error>) {
        switch result {
        case .success(let value):
            continuation.resume(returning: value)
        case .failure(let error):
            continuation.resume(throwing: error)
        }
    }
}

@MainActor
public protocol SpacetimeClientDelegate: AnyObject {
    func onConnect()
    func onDisconnect(error: Error?)
    func onConnectError(error: Error)
    func onConnectionStateChange(state: ConnectionState)
    func onIdentityReceived(identity: [UInt8], token: String)
    func onTransactionUpdate(message: Data?)
    func onReducerError(reducer: String, message: String, isInternal: Bool)
}

public extension SpacetimeClientDelegate {
    func onConnectError(error: Error) {}
    func onConnectionStateChange(state: ConnectionState) {}
    func onReducerError(reducer: String, message: String, isInternal: Bool) {}
}

public final class SpacetimeClient: @unchecked Sendable {
    public let serverUrl: URL
    public let moduleName: String
    
    private let stateLock: Mutex<Void> = Mutex(())
    @inline(__always)
    private func withStateLock<R>(_ body: () throws -> R) rethrows -> R {
        try stateLock.withLock { _ in
            try body()
        }
    }

    private weak var _delegate: SpacetimeClientDelegate?
    public var delegate: SpacetimeClientDelegate? {
        get { withStateLock { _delegate } }
        set { withStateLock { _delegate = newValue } }
    }
    
    private var _connectionState: ConnectionState = .disconnected
    public var connectionState: ConnectionState {
        withStateLock { _connectionState }
    }

    private static let sharedStateLock: Mutex<Void> = Mutex(())
    @inline(__always)
    private static func withSharedStateLock<R>(_ body: () throws -> R) rethrows -> R {
        try sharedStateLock.withLock { _ in
            try body()
        }
    }
    nonisolated(unsafe) private static var _shared: SpacetimeClient?
    public static var shared: SpacetimeClient? {
        get { withSharedStateLock { _shared } }
        set { withSharedStateLock { _shared = newValue } }
    }
    
    nonisolated(unsafe) public static var clientCache = ClientCache()

    private var webSocketTask: URLSessionWebSocketTask?
    private let urlSession: URLSession
    private let reconnectPolicy: ReconnectPolicy?
    private let compressionMode: CompressionMode
    private var savedToken: String?
    private var reconnectAttempt = 0
    private var shouldStayConnected = false

    private let encoder = BSATNEncoder()
    private let decoder = BSATNDecoder()
    private var nextRequestId = RequestId(rawValue: 1)
    private var nextQuerySetId = QuerySetId(rawValue: 1)
    private var pendingReducerNames: [RequestId: String] = [:]
    private var pendingProcedureCallbacks: [RequestId: (Result<Data, Error>) -> Void] = [:]
    private var pendingOneOffQueryCallbacks: [RequestId: (Result<QueryRows, Error>) -> Void] = [:]
    private var pendingSubscriptionByRequestId: [RequestId: SubscriptionHandle] = [:]
    private var activeSubscriptionByQuerySetId: [QuerySetId: SubscriptionHandle] = [:]
    private var pendingUnsubscribeByRequestId: [RequestId: SubscriptionHandle] = [:]
    private var managedSubscriptions: [ObjectIdentifier: SubscriptionHandle] = [:]
    private let decodeQueue = DispatchQueue(label: "spacetimedb.client.decode", qos: .utility)

    // Send queue — URLSessionWebSocketTask only supports one pending send at a time.
    private var sendQueue: [URLSessionWebSocketTask.Message] = []
    private var isSending = false
    private var networkMonitor: NetworkMonitor?
    private let keepAlivePingInterval: Duration
    private let keepAlivePongTimeout: Duration
    private var keepAliveTask: Task<Void, Never>?
    private var keepAliveTimeoutTask: Task<Void, Never>?
    private var awaitingKeepAlivePong = false
    private var reconnectTask: Task<Void, Never>?
    private var isHandlingConnectionFailure = false

    public init(
        serverUrl: URL,
        moduleName: String,
        reconnectPolicy: ReconnectPolicy? = ReconnectPolicy(),
        compressionMode: CompressionMode = .gzip,
        keepAlivePingIntervalSeconds: TimeInterval = 30.0,
        keepAlivePongTimeoutSeconds: TimeInterval = 10.0
    ) {
        self.serverUrl = serverUrl
        self.moduleName = moduleName
        self.reconnectPolicy = reconnectPolicy
        self.compressionMode = compressionMode
        let boundedPingInterval = max(1.0, keepAlivePingIntervalSeconds)
        let boundedPongTimeout = max(1.0, min(keepAlivePongTimeoutSeconds, boundedPingInterval))
        self.keepAlivePingInterval = .milliseconds(Int64((boundedPingInterval * 1000).rounded()))
        self.keepAlivePongTimeout = .milliseconds(Int64((boundedPongTimeout * 1000).rounded()))
        let config = URLSessionConfiguration.ephemeral
        config.httpShouldSetCookies = false
        config.httpCookieAcceptPolicy = .never
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        self.urlSession = URLSession(configuration: config)
    }

    public func connect(token: String? = nil) {
        withStateLock {
            shouldStayConnected = true
            reconnectAttempt = 0
            if let token {
                self.savedToken = token
            }
            reconnectTask?.cancel()
            reconnectTask = nil
        }
        
        startNetworkMonitor()
        performConnect(authToken: token ?? getSavedToken(), isReconnect: false)
    }
    
    private func getSavedToken() -> String? {
        withStateLock { savedToken }
    }

    private func performConnect(authToken: String?, isReconnect: Bool) {
        withStateLock {
            reconnectTask?.cancel()
            reconnectTask = nil
            isHandlingConnectionFailure = false
        }
        
        stopKeepAliveLoop()
        emitCounter(
            "spacetimedb.connection.attempts",
            tags: ["reconnect": isReconnect ? "true" : "false"]
        )
        var components = URLComponents(url: serverUrl, resolvingAgainstBaseURL: false)!
        components.path = "/v1/database/\(moduleName)/subscribe"
        components.queryItems = [URLQueryItem(name: "compression", value: compressionMode.queryValue)]
        if components.scheme == "http" { components.scheme = "ws" }
        if components.scheme == "https" { components.scheme = "wss" }

        var request = URLRequest(url: components.url!)
        request.setValue("v2.bsatn.spacetimedb", forHTTPHeaderField: "Sec-WebSocket-Protocol")
        if let token = authToken {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        let (procedureCallbacks, queryCallbacks) = withStateLock { () -> ([RequestId: (Result<Data, Error>) -> Void], [RequestId: (Result<QueryRows, Error>) -> Void]) in
            sendQueue.removeAll()
            isSending = false
            nextRequestId = RequestId(rawValue: 1)
            nextQuerySetId = QuerySetId(rawValue: 1)
            pendingReducerNames.removeAll()
            let procedureCallbacks = pendingProcedureCallbacks
            pendingProcedureCallbacks.removeAll()
            let queryCallbacks = pendingOneOffQueryCallbacks
            pendingOneOffQueryCallbacks.removeAll()
            pendingSubscriptionByRequestId.removeAll()
            pendingUnsubscribeByRequestId.removeAll()
            activeSubscriptionByQuerySetId.removeAll()
            return (procedureCallbacks, queryCallbacks)
        }
        
        failCallbacks(procedureCallbacks: procedureCallbacks, queryCallbacks: queryCallbacks, error: SpacetimeClientProcedureError.disconnected)
        
        setConnectionState(isReconnect ? .reconnecting : .connecting)
        
        let task = withStateLock { () -> URLSessionWebSocketTask? in
            webSocketTask = urlSession.webSocketTask(with: request)
            return webSocketTask
        }
        
        task?.resume()
        receiveMessage()
    }

    public func disconnect() {
        let (task, procedureCallbacks, queryCallbacks, managed) = withStateLock {
            shouldStayConnected = false
            reconnectTask?.cancel()
            reconnectTask = nil
            sendQueue.removeAll()
            isSending = false
            let task = webSocketTask
            webSocketTask = nil
            pendingReducerNames.removeAll()
            let procedureCallbacks = pendingProcedureCallbacks
            pendingProcedureCallbacks.removeAll()
            let queryCallbacks = pendingOneOffQueryCallbacks
            pendingOneOffQueryCallbacks.removeAll()
            pendingSubscriptionByRequestId.removeAll()
            pendingUnsubscribeByRequestId.removeAll()
            activeSubscriptionByQuerySetId.removeAll()
            let managed = managedSubscriptions.values
            managedSubscriptions.removeAll()
            return (task, procedureCallbacks, queryCallbacks, managed)
        }
        
        stopNetworkMonitor()
        stopKeepAliveLoop()
        
        if let task = task {
            switch task.state {
            case .running, .suspended:
                task.cancel(with: .normalClosure, reason: nil)
            case .canceling, .completed:
                break
            @unknown default:
                task.cancel(with: .normalClosure, reason: nil)
            }
        }
        
        failCallbacks(procedureCallbacks: procedureCallbacks, queryCallbacks: queryCallbacks, error: SpacetimeClientProcedureError.disconnected)
        
        for handle in managed {
            handle.markEnded()
        }
        
        setConnectionState(.disconnected)
        invokeDelegateCallback(named: "delegate.on_disconnect") { $0.onDisconnect(error: nil) }
    }
    
    private func failCallbacks(procedureCallbacks: [RequestId: (Result<Data, Error>) -> Void], queryCallbacks: [RequestId: (Result<QueryRows, Error>) -> Void], error: Error) {
        for callback in procedureCallbacks.values {
            callback(.failure(error))
        }
        for callback in queryCallbacks.values {
            callback(.failure(error))
        }
    }

    // MARK: - Serialized send queue

    private func enqueue(_ message: URLSessionWebSocketTask.Message) {
        withStateLock {
            sendQueue.append(message)
        }
        flushQueue()
    }

    private func flushQueue() {
        let next: (URLSessionWebSocketTask.Message, URLSessionWebSocketTask)? = withStateLock {
            guard !isSending, !sendQueue.isEmpty, let task = webSocketTask else {
                return nil
            }
            let msg = sendQueue.removeFirst()
            isSending = true
            return (msg, task)
        }
        guard let next else { return }
        let (msg, task) = next
        
        emitCounter("spacetimedb.messages.out.count")
        switch msg {
        case .data(let data):
            emitCounter("spacetimedb.messages.out.bytes", by: Int64(data.count))
        case .string(let text):
            emitCounter("spacetimedb.messages.out.bytes", by: Int64(text.utf8.count))
        @unknown default:
            break
        }
        
        task.send(msg) { [weak self] error in
            guard let self else { return }
            self.withStateLock {
                self.isSending = false
            }
            if let error = error {
                Log.network.error("Send error: \(error.localizedDescription)")
            }
            self.flushQueue()
        }
    }

    public func send<T: Encodable>(_ message: T) {
        do {
            let data = try encoder.encode(message)
            enqueue(.data(data))
        } catch {
            Log.network.error("Failed to encode message: \(error.localizedDescription)")
        }
    }

    public func send(_ reducerName: String, _ args: Data) {
        let requestId = allocateRequestId()
        withStateLock {
            pendingReducerNames[requestId] = reducerName
        }
        let call = CallReducer(requestId: requestId, flags: 0, reducer: reducerName, args: args)
        let message = ClientMessage.callReducer(call)
        send(message)
    }

    public func sendProcedure(_ procedureName: String, _ args: Data) {
        let call = CallProcedure(requestId: allocateRequestId(), flags: 0, procedure: procedureName, args: args)
        let message = ClientMessage.callProcedure(call)
        send(message)
    }

    public func sendProcedure(
        _ procedureName: String,
        _ args: Data,
        completion: @escaping (Result<Data, Error>) -> Void
    ) {
        sendProcedure(procedureName, args, decodeReturn: { $0 }, completion: completion)
    }

    public func sendProcedure<R>(
        _ procedureName: String,
        _ args: Data,
        decodeReturn: @escaping (Data) throws -> R,
        completion: @escaping (Result<R, Error>) -> Void
    ) {
        let requestId = allocateRequestId()
        let timedCallback = makeTimedCallback(named: "procedure.completion") { (result: Result<Data, Error>) in
            switch result {
            case .success(let data):
                do {
                    completion(.success(try decodeReturn(data)))
                } catch {
                    completion(.failure(error))
                }
            case .failure(let error):
                completion(.failure(error))
            }
        }
        
        withStateLock {
            pendingProcedureCallbacks[requestId] = timedCallback
        }

        let call = CallProcedure(requestId: requestId, flags: 0, procedure: procedureName, args: args)
        let message = ClientMessage.callProcedure(call)
        send(message)
    }

    public func sendProcedure<R: Decodable>(
        _ procedureName: String,
        _ args: Data,
        responseType: R.Type,
        completion: @escaping (Result<R, Error>) -> Void
    ) {
        sendProcedure(
            procedureName,
            args,
            decodeReturn: { [decoder] data in
                try decoder.decode(responseType, from: data)
            },
            completion: completion
        )
    }

    public func sendProcedure(
        _ procedureName: String,
        _ args: Data
    ) async throws -> Data {
        try await sendProcedure(procedureName, args, timeout: nil)
    }

    public func sendProcedure(
        _ procedureName: String,
        _ args: Data,
        timeout: Duration?
    ) async throws -> Data {
        try Task.checkCancellation()
        let requestId = allocateRequestId()
        let asyncResponse = AsyncResponseContinuation<Data>()

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                asyncResponse.install(continuation)
                guard !Task.isCancelled else {
                    asyncResponse.resolve(.failure(CancellationError()))
                    return
                }

                let timedCallback = makeTimedCallback(named: "procedure.completion") { result in
                    asyncResponse.resolve(result)
                }
                
                withStateLock {
                    pendingProcedureCallbacks[requestId] = timedCallback
                }

                if let timeout {
                    asyncResponse.setTimeoutTask(
                        Task { [weak self] in
                            try? await Task.sleep(for: timeout)
                            guard let self else { return }
                            let removed = self.withStateLock {
                                self.pendingProcedureCallbacks.removeValue(forKey: requestId) != nil
                            }
                            if removed {
                                asyncResponse.resolve(.failure(SpacetimeClientProcedureError.timeout))
                            }
                        }
                    )
                }

                let call = CallProcedure(requestId: requestId, flags: 0, procedure: procedureName, args: args)
                let message = ClientMessage.callProcedure(call)
                send(message)
            }
        } onCancel: {
            withStateLock {
                _ = self.pendingProcedureCallbacks.removeValue(forKey: requestId)
            }
            asyncResponse.resolve(.failure(CancellationError()))
        }
    }

    public func sendProcedure<R: Decodable & Sendable>(
        _ procedureName: String,
        _ args: Data,
        responseType: R.Type
    ) async throws -> R {
        try await sendProcedure(procedureName, args, responseType: responseType, timeout: nil)
    }

    public func sendProcedure<R: Decodable & Sendable>(
        _ procedureName: String,
        _ args: Data,
        responseType: R.Type,
        timeout: Duration?
    ) async throws -> R {
        let raw = try await sendProcedure(procedureName, args, timeout: timeout)
        return try decoder.decode(responseType, from: raw)
    }

    public func oneOffQuery(_ query: String, completion: @escaping (Result<QueryRows, Error>) -> Void) {
        let requestId = allocateRequestId()
        let timedCallback = makeTimedCallback(named: "one_off_query.completion", completion)
        withStateLock {
            pendingOneOffQueryCallbacks[requestId] = timedCallback
        }
        send(ClientMessage.oneOffQuery(OneOffQuery(requestId: requestId, queryString: query)))
    }

    public func oneOffQuery(_ query: String) async throws -> QueryRows {
        try await oneOffQuery(query, timeout: nil)
    }

    public func oneOffQuery(_ query: String, timeout: Duration?) async throws -> QueryRows {
        try Task.checkCancellation()
        let requestId = allocateRequestId()
        let asyncResponse = AsyncResponseContinuation<QueryRows>()

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                asyncResponse.install(continuation)
                guard !Task.isCancelled else {
                    asyncResponse.resolve(.failure(CancellationError()))
                    return
                }

                let timedCallback = makeTimedCallback(named: "one_off_query.completion") { result in
                    asyncResponse.resolve(result)
                }
                
                withStateLock {
                    pendingOneOffQueryCallbacks[requestId] = timedCallback
                }

                if let timeout {
                    asyncResponse.setTimeoutTask(
                        Task { [weak self] in
                            try? await Task.sleep(for: timeout)
                            guard let self else { return }
                            let removed = self.withStateLock {
                                self.pendingOneOffQueryCallbacks.removeValue(forKey: requestId) != nil
                            }
                            if removed {
                                asyncResponse.resolve(.failure(SpacetimeClientQueryError.timeout))
                            }
                        }
                    )
                }

                send(ClientMessage.oneOffQuery(OneOffQuery(requestId: requestId, queryString: query)))
            }
        } onCancel: {
            withStateLock {
                _ = self.pendingOneOffQueryCallbacks.removeValue(forKey: requestId)
            }
            asyncResponse.resolve(.failure(CancellationError()))
        }
    }

    public func subscribe(
        queries: [String],
        onApplied: (() -> Void)? = nil,
        onError: ((String) -> Void)? = nil
    ) -> SubscriptionHandle {
        let timedOnApplied = onApplied.map { callback in
            makeTimedVoidCallback(named: "subscription.on_applied", callback)
        }
        let timedOnError = onError.map { callback in
            makeTimedCallback(named: "subscription.on_error", callback)
        }
        let handle = SubscriptionHandle(queries: queries, client: self, onApplied: timedOnApplied, onError: timedOnError)
        withStateLock {
            managedSubscriptions[ObjectIdentifier(handle)] = handle
        }
        startSubscription(handle)
        return handle
    }

    public func unsubscribe(_ handle: SubscriptionHandle, sendDroppedRows: Bool = false) {
        guard handle.state == .active, let querySetId = handle.querySetId else {
            return
        }

        let flags: UInt8 = sendDroppedRows ? 1 : 0
        let requestId = allocateRequestId()
        withStateLock {
            pendingUnsubscribeByRequestId[requestId] = handle
        }
        send(ClientMessage.unsubscribe(Unsubscribe(requestId: requestId, querySetId: querySetId, flags: flags)))
    }

    public func subscribeAll(tables: [String]) {
        guard !tables.isEmpty else {
            return
        }
        let queries = tables.map { "SELECT * FROM \($0)" }
        let sub = Subscribe(
            queryStrings: queries,
            requestId: allocateRequestId(),
            querySetId: allocateQuerySetId()
        )
        let message = ClientMessage.subscribe(sub)
        send(message)
    }

    private func startSubscription(_ handle: SubscriptionHandle) {
        guard !handle.queries.isEmpty else {
            handle.markError("Subscription requires at least one query.")
            _ = withStateLock {
                managedSubscriptions.removeValue(forKey: ObjectIdentifier(handle))
            }
            return
        }

        let requestId = allocateRequestId()
        let querySetId = allocateQuerySetId()
        handle.markPending(requestId: requestId, querySetId: querySetId)
        withStateLock {
            pendingSubscriptionByRequestId[requestId] = handle
        }
        send(ClientMessage.subscribe(Subscribe(queryStrings: handle.queries, requestId: requestId, querySetId: querySetId)))
    }

    private func allocateRequestId() -> RequestId {
        withStateLock {
            let id = nextRequestId
            nextRequestId = RequestId(rawValue: nextRequestId.rawValue &+ 1)
            return id
        }
    }

    private func allocateQuerySetId() -> QuerySetId {
        withStateLock {
            let id = nextQuerySetId
            nextQuerySetId = QuerySetId(rawValue: nextQuerySetId.rawValue &+ 1)
            return id
        }
    }

    // MARK: - Receive loop

    private func receiveMessage() {
        let task = withStateLock { webSocketTask }
        
        task?.receive { [weak self] result in
            guard let self = self else { return }
            switch result {
            case .failure(let error):
                self.handleConnectionFailure(error)
            case .success(let message):
                switch message {
                case .data(let data):
                    self.emitCounter("spacetimedb.messages.in.count")
                    self.emitCounter("spacetimedb.messages.in.bytes", by: Int64(data.count))
                    
                    self.decodeQueue.async { [weak self] in
                        guard let self = self else { return }
                        let decoded = Self.decodeServerMessage(from: data)
                        self.handleDecodedServerMessage(decoded)
                    }
                case .string:
                    break
                @unknown default:
                    break
                }
                self.receiveMessage()
            }
        }
    }

    // MARK: - Message handling

    nonisolated private static func decodeServerMessage(from data: Data) -> Result<ServerMessage, Error> {
        do {
            let bsatnData = try ServerMessageFrameDecoder.decodePayload(from: data)
            let decoder = BSATNDecoder()
            return .success(try decoder.decode(ServerMessage.self, from: bsatnData))
        } catch {
            return .failure(error)
        }
    }

    private func handleDecodedServerMessage(_ decoded: Result<ServerMessage, Error>) {
        switch decoded {
        case .failure(let error):
            Log.network.error("Failed to decode server message: \(error.localizedDescription)")
        case .success(let serverMsg):
            switch serverMsg {
            case .initialConnection(let connection):
                withStateLock {
                    reconnectAttempt = 0
                    savedToken = connection.token
                }
                
                setConnectionState(.connected)
                startKeepAliveLoop()
                invokeDelegateCallback(named: "delegate.on_identity_received") {
                    $0.onIdentityReceived(identity: Array(connection.identity.rawBytes), token: connection.token)
                }
                subscribeAll(tables: Self.clientCache.registeredTableNames)
                resubscribeManagedSubscriptions()
                invokeDelegateCallback(named: "delegate.on_connect") { $0.onConnect() }
            case .transactionUpdate(let update):
                Self.clientCache.applyTransactionUpdate(update)
                invokeDelegateCallback(named: "delegate.on_transaction_update") { $0.onTransactionUpdate(message: nil) }
            case .subscribeApplied(let applied):
                handleSubscribeApplied(applied)
                let initial = applied.asTransactionUpdate()
                Self.clientCache.applyTransactionUpdate(initial)
                invokeDelegateCallback(named: "delegate.on_transaction_update") { $0.onTransactionUpdate(message: nil) }
            case .reducerResult(let reducerResult):
                handleReducerResult(reducerResult)
            case .other:
                break
            case .subscriptionError(let error):
                Log.client.warning("Subscription error for query_set_id=\(error.querySetId): \(error.error)")
                handleSubscriptionError(error)
            case .procedureResult(let result):
                handleProcedureResult(result)
            case .unsubscribeApplied(let applied):
                handleUnsubscribeApplied(applied)
            case .oneOffQueryResult(let result):
                handleOneOffQueryResult(result)
            }
        }
    }

    func handleReducerResult(_ reducerResult: ReducerResult) {
        let reducerName = withStateLock {
            pendingReducerNames.removeValue(forKey: reducerResult.requestId) ?? "<unknown>"
        }
        
        switch reducerResult.result {
        case .ok(let ok):
            Self.clientCache.applyTransactionUpdate(ok.transactionUpdate)
            invokeDelegateCallback(named: "delegate.on_transaction_update") { $0.onTransactionUpdate(message: nil) }
        case .okEmpty:
            break
        case .err(let errData):
            let message: String
            if let decoded = try? decoder.decode(String.self, from: errData) {
                message = decoded
            } else if let utf8 = String(data: errData, encoding: .utf8), !utf8.isEmpty {
                message = utf8
            } else {
                message = "non-text payload (\(errData.count) bytes)"
            }
            Log.client.warning("Reducer request_id=\(reducerResult.requestId) returned error: \(message)")
            invokeDelegateCallback(named: "delegate.on_reducer_error") {
                $0.onReducerError(reducer: reducerName, message: message, isInternal: false)
            }
        case .internalError(let message):
            Log.client.error("Reducer request_id=\(reducerResult.requestId) internal error: \(message)")
            invokeDelegateCallback(named: "delegate.on_reducer_error") {
                $0.onReducerError(reducer: reducerName, message: message, isInternal: true)
            }
            break
        }
    }

    func handleProcedureResult(_ result: ProcedureResult) {
        let callback = withStateLock {
            pendingProcedureCallbacks.removeValue(forKey: result.requestId)
        }
        
        guard let callback else {
            Log.client.warning("Received ProcedureResult for unknown request_id: \(result.requestId)")
            return
        }

        switch result.status {
        case .returned(let data):
            callback(.success(data))
        case .internalError(let message):
            callback(.failure(SpacetimeClientProcedureError.internalError(message)))
        }
    }

    func handleOneOffQueryResult(_ result: OneOffQueryResult) {
        let callback = withStateLock {
            pendingOneOffQueryCallbacks.removeValue(forKey: result.requestId)
        }
        
        guard let callback else {
            Log.client.warning("Received OneOffQueryResult for unknown request_id: \(result.requestId)")
            return
        }

        switch result.result {
        case .ok(let rows):
            callback(.success(rows))
        case .err(let message):
            callback(.failure(SpacetimeClientQueryError.serverError(message)))
        }
    }

    func handleSubscribeApplied(_ applied: SubscribeApplied) {
        let handle = withStateLock { () -> SubscriptionHandle? in
            let handle = pendingSubscriptionByRequestId.removeValue(forKey: applied.requestId)
            if let handle {
                activeSubscriptionByQuerySetId[applied.querySetId] = handle
            }
            return handle
        }
        
        handle?.markApplied(querySetId: applied.querySetId)
    }

    func handleUnsubscribeApplied(_ applied: UnsubscribeApplied) {
        let handle = withStateLock { () -> SubscriptionHandle? in
            let handle = pendingUnsubscribeByRequestId.removeValue(forKey: applied.requestId)
            if let handle {
                activeSubscriptionByQuerySetId.removeValue(forKey: applied.querySetId)
                managedSubscriptions.removeValue(forKey: ObjectIdentifier(handle))
            }
            return handle
        }
        
        guard let handle else { return }
        handle.markEnded()

        if let rows = applied.rows {
            let update = queryRowsToTransactionUpdate(rows, querySetId: applied.querySetId, asInserts: false)
            Self.clientCache.applyTransactionUpdate(update)
            invokeDelegateCallback(named: "delegate.on_transaction_update") { $0.onTransactionUpdate(message: nil) }
        }
    }

    func handleSubscriptionError(_ error: SubscriptionError) {
        let handle: SubscriptionHandle? = withStateLock {
            if let requestId = error.requestId, let pending = pendingSubscriptionByRequestId.removeValue(forKey: requestId) {
                managedSubscriptions.removeValue(forKey: ObjectIdentifier(pending))
                return pending
            }
            let active = activeSubscriptionByQuerySetId.removeValue(forKey: error.querySetId)
            if let active {
                managedSubscriptions.removeValue(forKey: ObjectIdentifier(active))
            }
            return active
        }

        handle?.markError(error.error)
    }

    private func resubscribeManagedSubscriptions() {
        let subsToStart = withStateLock {
            activeSubscriptionByQuerySetId.removeAll()
            pendingSubscriptionByRequestId.removeAll()
            return managedSubscriptions.values.filter { $0.state != .ended }
        }
        
        for handle in subsToStart {
            startSubscription(handle)
        }
    }

    private func queryRowsToTransactionUpdate(_ rows: QueryRows, querySetId: QuerySetId, asInserts: Bool) -> TransactionUpdate {
        let updates = rows.tables.map { tableRows in
            let persistent = PersistentTableRows(
                inserts: asInserts ? tableRows.rows : .empty,
                deletes: asInserts ? .empty : tableRows.rows
            )
            return TableUpdate(tableName: tableRows.table, rows: [.persistentTable(persistent)])
        }

        return TransactionUpdate(querySets: [QuerySetUpdate(querySetId: querySetId, tables: updates)])
    }

    // MARK: - Network monitoring

    private func startNetworkMonitor() {
        let monitor = withStateLock { () -> NetworkMonitor? in
            guard networkMonitor == nil else {
                return nil
            }
            return NetworkMonitor()
        }
        guard let monitor else { return }
        
        monitor.onPathChange = { [weak self] isConnected in
            guard let self else { return }
            let shouldConnected = self.withStateLock { self.shouldStayConnected }
            let state = self.withStateLock { self._connectionState }
            let token = self.withStateLock { self.savedToken }
            
            if isConnected && shouldConnected {
                Log.network.info("Network restored, attempting reconnect")
                self.withStateLock {
                    self.reconnectAttempt = 0
                }
                
                guard state != .connected else { return }
                self.performConnect(authToken: token, isReconnect: true)
            }
        }
        monitor.start()
        
        withStateLock {
            networkMonitor = monitor
        }
    }

    private func stopNetworkMonitor() {
        let monitor = withStateLock { () -> NetworkMonitor? in
            let monitor = networkMonitor
            networkMonitor = nil
            return monitor
        }
        monitor?.stop()
    }

    // MARK: - Connection lifecycle

    private func setConnectionState(_ state: ConnectionState) {
        let changed = withStateLock { () -> Bool in
            guard _connectionState != state else { return false }
            _connectionState = state
            return true
        }
        guard changed else { return }
        
        emitGauge(
            "spacetimedb.connection.state",
            value: stateMetricValue(state),
            tags: ["state": stateMetricName(state)]
        )
        invokeDelegateCallback(named: "delegate.on_connection_state_change") {
            $0.onConnectionStateChange(state: state)
        }
    }

    private func handleConnectionFailure(_ error: Error) {
        let failureState = withStateLock { () -> (URLSessionWebSocketTask?, ConnectionState, [RequestId: (Result<Data, Error>) -> Void], [RequestId: (Result<QueryRows, Error>) -> Void])? in
            guard shouldStayConnected else { return nil }
            guard !isHandlingConnectionFailure else { return nil }
            isHandlingConnectionFailure = true
            let task = webSocketTask
            webSocketTask = nil
            let state = _connectionState
            let procCallbacks = pendingProcedureCallbacks
            pendingProcedureCallbacks.removeAll()
            let queryCallbacks = pendingOneOffQueryCallbacks
            pendingOneOffQueryCallbacks.removeAll()
            return (task, state, procCallbacks, queryCallbacks)
        }
        guard let (task, state, procCallbacks, queryCallbacks) = failureState else { return }

        Log.network.error("WebSocket error: \(error.localizedDescription)")
        emitCounter(
            "spacetimedb.connection.failures",
            tags: ["state": stateMetricName(state)]
        )
        stopKeepAliveLoop()
        task?.cancel(with: .goingAway, reason: nil)

        failCallbacks(procedureCallbacks: procCallbacks, queryCallbacks: queryCallbacks, error: error)

        if state == .connecting {
            invokeDelegateCallback(named: "delegate.on_connect_error") { @MainActor in $0.onConnectError(error: error) }
        }
        invokeDelegateCallback(named: "delegate.on_disconnect") { @MainActor in $0.onDisconnect(error: error) }

        guard let reconnectDelay = nextReconnectDelay() else {
            withStateLock {
                shouldStayConnected = false
            }
            setConnectionState(.disconnected)
            withStateLock {
                isHandlingConnectionFailure = false
            }
            return
        }

        setConnectionState(.reconnecting)

        let connected = withStateLock { networkMonitor?.isConnected ?? true }
        
        // If network is unavailable, defer reconnection until path restores.
        guard connected else {
            Log.network.info("Network unavailable, deferring reconnect until path restores")
            withStateLock {
                isHandlingConnectionFailure = false
            }
            return
        }

        withStateLock {
            reconnectTask?.cancel()
            reconnectTask = Task { [weak self] in
                try? await Task.sleep(for: reconnectDelay)
                guard let self else { return }
                let (shouldStay, token) = self.withStateLock {
                    (self.shouldStayConnected, self.savedToken)
                }
                guard shouldStay else {
                    self.withStateLock { self.isHandlingConnectionFailure = false }
                    return
                }
                self.withStateLock { self.isHandlingConnectionFailure = false }
                self.performConnect(authToken: token, isReconnect: true)
            }
        }
    }

    // MARK: - Keepalive

    private func startKeepAliveLoop() {
        stopKeepAliveLoop()
        withStateLock {
            keepAliveTask = Task { [weak self] in
                while !Task.isCancelled {
                    guard let self else { return }
                    let interval = self.withStateLock { self.keepAlivePingInterval }
                    
                    try? await Task.sleep(for: interval)
                    guard !Task.isCancelled else { return }
                    self.sendKeepAlivePing()
                }
            }
        }
    }

    private func stopKeepAliveLoop() {
        withStateLock {
            keepAliveTask?.cancel()
            keepAliveTask = nil
            keepAliveTimeoutTask?.cancel()
            keepAliveTimeoutTask = nil
            awaitingKeepAlivePong = false
        }
    }

    private func sendKeepAlivePing() {
        let taskOrTimeout = withStateLock { () -> (URLSessionWebSocketTask?, Bool) in
            guard shouldStayConnected, _connectionState == .connected, let task = webSocketTask else {
                return (nil, false)
            }
            guard !awaitingKeepAlivePong else {
                return (nil, true)
            }
            return (task, false)
        }
        if taskOrTimeout.1 {
            handleConnectionFailure(SpacetimeClientConnectionError.keepAliveTimeout)
            return
        }
        guard let task = taskOrTimeout.0 else { return }

        withStateLock {
            awaitingKeepAlivePong = true
            keepAliveTimeoutTask?.cancel()
            let timeout = keepAlivePongTimeout
            keepAliveTimeoutTask = Task { [weak self] in
                try? await Task.sleep(for: timeout)
                guard let self else { return }
                let awaiting = self.withStateLock {
                    let val = self.awaitingKeepAlivePong
                    self.awaitingKeepAlivePong = false
                    return val
                }
                if awaiting {
                    self.handleConnectionFailure(SpacetimeClientConnectionError.keepAliveTimeout)
                }
            }
        }
        task.sendPing { [weak self] error in
            guard let self else { return }
            self.withStateLock {
                self.awaitingKeepAlivePong = false
                self.keepAliveTimeoutTask?.cancel()
                self.keepAliveTimeoutTask = nil
            }
            if let error {
                self.handleConnectionFailure(error)
            }
        }
    }

    private func nextReconnectDelay() -> Duration? {
        withStateLock {
            guard let reconnectPolicy else { return nil }
            reconnectAttempt += 1
            return reconnectPolicy.delay(forAttempt: reconnectAttempt)
        }
    }

    private func emitCounter(_ name: String, by value: Int64 = 1, tags: [String: String] = [:]) {
        SpacetimeObservability.metrics.incrementCounter(name, by: value, tags: tags)
    }

    private func emitGauge(_ name: String, value: Double, tags: [String: String] = [:]) {
        SpacetimeObservability.metrics.recordGauge(name, value: value, tags: tags)
    }

    private func emitTiming(_ name: String, milliseconds: Double, tags: [String: String] = [:]) {
        SpacetimeObservability.metrics.recordTiming(name, milliseconds: milliseconds, tags: tags)
    }

    private func invokeDelegateCallback(
        named callbackName: String,
        _ callback: @escaping @MainActor (SpacetimeClientDelegate) -> Void
    ) {
        let delegate = withStateLock { _delegate }
        
        guard let delegate else { return }
        Task { @MainActor in
            self.emitTimedCallbackMetric(named: callbackName) {
                callback(delegate)
            }
        }
    }

    private func makeTimedVoidCallback(
        named callbackName: String,
        _ callback: @escaping () -> Void
    ) -> (() -> Void) {
        { [weak self] in
            guard let self else {
                callback()
                return
            }
            self.emitTimedCallbackMetric(named: callbackName, callback)
        }
    }

    private func makeTimedCallback<T>(
        named callbackName: String,
        _ callback: @escaping (T) -> Void
    ) -> ((T) -> Void) {
        { [weak self] value in
            guard let self else {
                callback(value)
                return
            }
            self.emitTimedCallbackMetric(named: callbackName) {
                callback(value)
            }
        }
    }

    private func emitTimedCallbackMetric(named callbackName: String, _ callback: () -> Void) {
        let start = ContinuousClock.now
        callback()
        let elapsed = start.duration(to: ContinuousClock.now)
        let components = elapsed.components
        let milliseconds =
            (Double(components.seconds) * 1000)
            + (Double(components.attoseconds) / 1_000_000_000_000_000)
        emitTiming(
            "spacetimedb.callback.latency",
            milliseconds: milliseconds,
            tags: ["callback": callbackName]
        )
    }

    private func stateMetricName(_ state: ConnectionState) -> String {
        switch state {
        case .disconnected:
            return "disconnected"
        case .connecting:
            return "connecting"
        case .connected:
            return "connected"
        case .reconnecting:
            return "reconnecting"
        }
    }

    private func stateMetricValue(_ state: ConnectionState) -> Double {
        switch state {
        case .disconnected:
            return 0
        case .connecting:
            return 1
        case .connected:
            return 2
        case .reconnecting:
            return 3
        }
    }
}

#if DEBUG
extension SpacetimeClient {
    func _test_simulateConnectionFailure(_ error: Error, shouldStayConnected: Bool = true) {
        withStateLock {
            self.shouldStayConnected = shouldStayConnected
        }
        handleConnectionFailure(error)
    }

    func _test_deliverServerMessage(_ message: ServerMessage) {
        handleDecodedServerMessage(.success(message))
    }

    func _test_setConnectionState(_ state: ConnectionState) {
        setConnectionState(state)
    }

    func _test_pendingProcedureCallbackCount() -> Int {
        withStateLock { pendingProcedureCallbacks.count }
    }

    func _test_pendingOneOffQueryCallbackCount() -> Int {
        withStateLock { pendingOneOffQueryCallbacks.count }
    }
}
#endif
