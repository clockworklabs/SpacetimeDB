import Foundation

public enum SpacetimeClientProcedureError: Error, Equatable {
    case internalError(String)
    case disconnected
    case timeout
}

public enum SpacetimeClientConnectionError: Error, Equatable {
    case keepAliveTimeout
}

private final class AsyncResponseContinuation<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Value, Error>?
    private var timeoutTask: Task<Void, Never>?
    private var completionResult: Result<Value, Error>?
    private var isCompleted = false

    func install(_ continuation: CheckedContinuation<Value, Error>) {
        var resultToResume: Result<Value, Error>?

        lock.lock()
        if isCompleted {
            resultToResume = completionResult
            completionResult = nil
        } else {
            self.continuation = continuation
        }
        lock.unlock()

        if let resultToResume {
            resume(continuation, with: resultToResume)
        }
    }

    func setTimeoutTask(_ task: Task<Void, Never>) {
        var shouldCancelTask = false

        lock.lock()
        if isCompleted {
            shouldCancelTask = true
        } else {
            timeoutTask = task
        }
        lock.unlock()

        if shouldCancelTask {
            task.cancel()
        }
    }

    func resolve(_ result: Result<Value, Error>) {
        var continuationToResume: CheckedContinuation<Value, Error>?
        var timeoutTaskToCancel: Task<Void, Never>?

        lock.lock()
        if isCompleted {
            lock.unlock()
            return
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
        lock.unlock()

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

@MainActor
public final class SpacetimeClient: @unchecked Sendable {
    public let serverUrl: URL
    public let moduleName: String
    public weak var delegate: SpacetimeClientDelegate?
    public private(set) var connectionState: ConnectionState = .disconnected

    public static var shared: SpacetimeClient?
    public static var clientCache = ClientCache()

    private var webSocketTask: URLSessionWebSocketTask?
    private let urlSession: URLSession
    private let reconnectPolicy: ReconnectPolicy?
    private let compressionMode: CompressionMode
    private var savedToken: String?
    private var reconnectAttempt = 0
    private var shouldStayConnected = false

    private let encoder = BSATNEncoder()
    private let decoder = BSATNDecoder()
    private var nextRequestId: UInt32 = 1
    private var nextQuerySetId: UInt32 = 1
    private var pendingReducerNames: [UInt32: String] = [:]
    private var pendingProcedureCallbacks: [UInt32: (Result<Data, Error>) -> Void] = [:]
    private var pendingOneOffQueryCallbacks: [UInt32: (Result<QueryRows, Error>) -> Void] = [:]
    private var pendingSubscriptionByRequestId: [UInt32: SubscriptionHandle] = [:]
    private var activeSubscriptionByQuerySetId: [UInt32: SubscriptionHandle] = [:]
    private var pendingUnsubscribeByRequestId: [UInt32: SubscriptionHandle] = [:]
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
        shouldStayConnected = true
        reconnectAttempt = 0
        if let token {
            self.savedToken = token
        }
        reconnectTask?.cancel()
        reconnectTask = nil
        startNetworkMonitor()
        performConnect(authToken: token ?? self.savedToken, isReconnect: false)
    }

    private func performConnect(authToken: String?, isReconnect: Bool) {
        reconnectTask?.cancel()
        reconnectTask = nil
        isHandlingConnectionFailure = false
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

        sendQueue.removeAll()
        isSending = false
        nextRequestId = 1
        nextQuerySetId = 1
        pendingReducerNames.removeAll()
        failPendingProcedureCallbacks(with: SpacetimeClientProcedureError.disconnected)
        failPendingOneOffQueryCallbacks(with: SpacetimeClientQueryError.disconnected)
        pendingSubscriptionByRequestId.removeAll()
        pendingUnsubscribeByRequestId.removeAll()
        activeSubscriptionByQuerySetId.removeAll()
        setConnectionState(isReconnect ? .reconnecting : .connecting)
        webSocketTask = urlSession.webSocketTask(with: request)
        webSocketTask?.resume()
        receiveMessage()
    }

    public func disconnect() {
        shouldStayConnected = false
        reconnectTask?.cancel()
        reconnectTask = nil
        stopNetworkMonitor()
        stopKeepAliveLoop()
        sendQueue.removeAll()
        isSending = false
        if let task = webSocketTask {
            switch task.state {
            case .running, .suspended:
                task.cancel(with: .normalClosure, reason: nil)
            case .canceling, .completed:
                break
            @unknown default:
                task.cancel(with: .normalClosure, reason: nil)
            }
        }
        webSocketTask = nil
        pendingReducerNames.removeAll()
        failPendingProcedureCallbacks(with: SpacetimeClientProcedureError.disconnected)
        failPendingOneOffQueryCallbacks(with: SpacetimeClientQueryError.disconnected)
        pendingSubscriptionByRequestId.removeAll()
        pendingUnsubscribeByRequestId.removeAll()
        activeSubscriptionByQuerySetId.removeAll()
        for handle in managedSubscriptions.values {
            handle.markEnded()
        }
        managedSubscriptions.removeAll()
        setConnectionState(.disconnected)
        invokeDelegateCallback(named: "delegate.on_disconnect") { $0.onDisconnect(error: nil) }
    }

    // MARK: - Serialized send queue

    private func enqueue(_ message: URLSessionWebSocketTask.Message) {
        sendQueue.append(message)
        flushQueue()
    }

    private func flushQueue() {
        guard !isSending, !sendQueue.isEmpty, let task = webSocketTask else { return }
        let msg = sendQueue.removeFirst()
        emitCounter("spacetimedb.messages.out.count")
        switch msg {
        case .data(let data):
            emitCounter("spacetimedb.messages.out.bytes", by: Int64(data.count))
        case .string(let text):
            emitCounter("spacetimedb.messages.out.bytes", by: Int64(text.utf8.count))
        @unknown default:
            break
        }
        isSending = true
        task.send(msg) { [weak self] error in
            Task { @MainActor [weak self] in
                self?.isSending = false
                if let error = error {
                    Log.network.error("Send error: \(error.localizedDescription)")
                }
                self?.flushQueue()
            }
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
        pendingReducerNames[requestId] = reducerName
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
        pendingProcedureCallbacks[requestId] = makeTimedCallback(named: "procedure.completion") { result in
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

                pendingProcedureCallbacks[requestId] = makeTimedCallback(named: "procedure.completion") { result in
                    asyncResponse.resolve(result)
                }

                if let timeout {
                    asyncResponse.setTimeoutTask(
                        Task { [weak self] in
                            try? await Task.sleep(for: timeout)
                            await MainActor.run {
                                guard let self else { return }
                                guard self.pendingProcedureCallbacks.removeValue(forKey: requestId) != nil else { return }
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
            Task { @MainActor [weak self] in
                guard let self else { return }
                _ = self.pendingProcedureCallbacks.removeValue(forKey: requestId)
                asyncResponse.resolve(.failure(CancellationError()))
            }
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
        pendingOneOffQueryCallbacks[requestId] = makeTimedCallback(named: "one_off_query.completion", completion)
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

                pendingOneOffQueryCallbacks[requestId] = makeTimedCallback(named: "one_off_query.completion") { result in
                    asyncResponse.resolve(result)
                }

                if let timeout {
                    asyncResponse.setTimeoutTask(
                        Task { [weak self] in
                            try? await Task.sleep(for: timeout)
                            await MainActor.run {
                                guard let self else { return }
                                guard self.pendingOneOffQueryCallbacks.removeValue(forKey: requestId) != nil else { return }
                                asyncResponse.resolve(.failure(SpacetimeClientQueryError.timeout))
                            }
                        }
                    )
                }

                send(ClientMessage.oneOffQuery(OneOffQuery(requestId: requestId, queryString: query)))
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                guard let self else { return }
                _ = self.pendingOneOffQueryCallbacks.removeValue(forKey: requestId)
                asyncResponse.resolve(.failure(CancellationError()))
            }
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
        managedSubscriptions[ObjectIdentifier(handle)] = handle
        startSubscription(handle)
        return handle
    }

    public func unsubscribe(_ handle: SubscriptionHandle, sendDroppedRows: Bool = false) {
        guard handle.state == .active, let querySetId = handle.querySetId else {
            return
        }

        let flags: UInt8 = sendDroppedRows ? 1 : 0
        let requestId = allocateRequestId()
        pendingUnsubscribeByRequestId[requestId] = handle
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
            managedSubscriptions.removeValue(forKey: ObjectIdentifier(handle))
            return
        }

        let requestId = allocateRequestId()
        let querySetId = allocateQuerySetId()
        handle.markPending(requestId: requestId, querySetId: querySetId)
        pendingSubscriptionByRequestId[requestId] = handle
        send(ClientMessage.subscribe(Subscribe(queryStrings: handle.queries, requestId: requestId, querySetId: querySetId)))
    }

    private func allocateRequestId() -> UInt32 {
        let id = nextRequestId
        nextRequestId &+= 1
        return id
    }

    private func allocateQuerySetId() -> UInt32 {
        let id = nextQuerySetId
        nextQuerySetId &+= 1
        return id
    }

    // MARK: - Receive loop

    private func receiveMessage() {
        webSocketTask?.receive { [weak self] result in
            guard let self = self else { return }
            Task { @MainActor in
                switch result {
                case .failure(let error):
                    self.handleConnectionFailure(error)
                case .success(let message):
                    switch message {
                    case .data(let data):
                        self.emitCounter("spacetimedb.messages.in.count")
                        self.emitCounter("spacetimedb.messages.in.bytes", by: Int64(data.count))
                        self.decodeQueue.async { [weak self] in
                            guard let self else { return }
                            let decoded = Self.decodeServerMessage(from: data)
                            Task { @MainActor [weak self] in
                                self?.handleDecodedServerMessage(decoded)
                            }
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
                reconnectAttempt = 0
                savedToken = connection.token
                setConnectionState(.connected)
                startKeepAliveLoop()
                invokeDelegateCallback(named: "delegate.on_identity_received") {
                    $0.onIdentityReceived(identity: Array(connection.identity), token: connection.token)
                }
                subscribeAll(tables: Array(Self.clientCache.registeredTableNames))
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
        let reducerName = pendingReducerNames.removeValue(forKey: reducerResult.requestId) ?? "<unknown>"
        switch reducerResult.result {
        case .ok(let ok):
            Self.clientCache.applyTransactionUpdate(ok.transactionUpdate)
            delegate?.onTransactionUpdate(message: nil)
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
        guard let callback = pendingProcedureCallbacks.removeValue(forKey: result.requestId) else {
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
        guard let callback = pendingOneOffQueryCallbacks.removeValue(forKey: result.requestId) else {
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
        guard let handle = pendingSubscriptionByRequestId.removeValue(forKey: applied.requestId) else {
            return
        }
        activeSubscriptionByQuerySetId[applied.querySetId] = handle
        handle.markApplied(querySetId: applied.querySetId)
    }

    func handleUnsubscribeApplied(_ applied: UnsubscribeApplied) {
        guard let handle = pendingUnsubscribeByRequestId.removeValue(forKey: applied.requestId) else {
            return
        }

        activeSubscriptionByQuerySetId.removeValue(forKey: applied.querySetId)
        managedSubscriptions.removeValue(forKey: ObjectIdentifier(handle))
        handle.markEnded()

        if let rows = applied.rows {
            let update = queryRowsToTransactionUpdate(rows, querySetId: applied.querySetId, asInserts: false)
            Self.clientCache.applyTransactionUpdate(update)
            invokeDelegateCallback(named: "delegate.on_transaction_update") { $0.onTransactionUpdate(message: nil) }
        }
    }

    func handleSubscriptionError(_ error: SubscriptionError) {
        let handle: SubscriptionHandle? = {
            if let requestId = error.requestId, let pending = pendingSubscriptionByRequestId.removeValue(forKey: requestId) {
                return pending
            }
            return activeSubscriptionByQuerySetId.removeValue(forKey: error.querySetId)
        }()

        guard let handle else { return }
        managedSubscriptions.removeValue(forKey: ObjectIdentifier(handle))
        handle.markError(error.error)
    }

    private func failPendingProcedureCallbacks(with error: Error) {
        guard !pendingProcedureCallbacks.isEmpty else { return }
        let callbacks = pendingProcedureCallbacks.values
        pendingProcedureCallbacks.removeAll()
        for callback in callbacks {
            callback(.failure(error))
        }
    }

    private func failPendingOneOffQueryCallbacks(with error: Error) {
        guard !pendingOneOffQueryCallbacks.isEmpty else { return }
        let callbacks = pendingOneOffQueryCallbacks.values
        pendingOneOffQueryCallbacks.removeAll()
        for callback in callbacks {
            callback(.failure(error))
        }
    }

    private func resubscribeManagedSubscriptions() {
        activeSubscriptionByQuerySetId.removeAll()
        pendingSubscriptionByRequestId.removeAll()
        for handle in managedSubscriptions.values where handle.state != .ended {
            startSubscription(handle)
        }
    }

    private func queryRowsToTransactionUpdate(_ rows: QueryRows, querySetId: UInt32, asInserts: Bool) -> TransactionUpdate {
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
        guard networkMonitor == nil else { return }
        let monitor = NetworkMonitor()
        monitor.onPathChange = { [weak self] isConnected in
            guard let self, self.shouldStayConnected else { return }
            if isConnected {
                Log.network.info("Network restored, attempting reconnect")
                self.reconnectAttempt = 0
                guard self.connectionState != .connected else { return }
                self.performConnect(authToken: self.savedToken, isReconnect: true)
            }
        }
        monitor.start()
        networkMonitor = monitor
    }

    private func stopNetworkMonitor() {
        networkMonitor?.stop()
        networkMonitor = nil
    }

    // MARK: - Connection lifecycle

    private func setConnectionState(_ state: ConnectionState) {
        guard connectionState != state else { return }
        connectionState = state
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
        guard shouldStayConnected else { return }
        guard !isHandlingConnectionFailure else { return }
        isHandlingConnectionFailure = true

        Log.network.error("WebSocket error: \(error.localizedDescription)")
        emitCounter(
            "spacetimedb.connection.failures",
            tags: ["state": stateMetricName(connectionState)]
        )
        stopKeepAliveLoop()
        webSocketTask?.cancel(with: .goingAway, reason: nil)
        webSocketTask = nil

        failPendingProcedureCallbacks(with: error)
        failPendingOneOffQueryCallbacks(with: error)

        if connectionState == .connecting {
            invokeDelegateCallback(named: "delegate.on_connect_error") { $0.onConnectError(error: error) }
        }
        invokeDelegateCallback(named: "delegate.on_disconnect") { $0.onDisconnect(error: error) }

        guard let reconnectDelay = nextReconnectDelay() else {
            shouldStayConnected = false
            setConnectionState(.disconnected)
            isHandlingConnectionFailure = false
            return
        }

        setConnectionState(.reconnecting)

        // If network is unavailable, defer reconnection until path restores.
        guard networkMonitor?.isConnected ?? true else {
            Log.network.info("Network unavailable, deferring reconnect until path restores")
            isHandlingConnectionFailure = false
            return
        }

        reconnectTask?.cancel()
        reconnectTask = Task { @MainActor [weak self] in
            guard let self else { return }
            try? await Task.sleep(for: reconnectDelay)
            guard self.shouldStayConnected else {
                self.isHandlingConnectionFailure = false
                return
            }
            self.isHandlingConnectionFailure = false
            self.performConnect(authToken: self.savedToken, isReconnect: true)
        }
    }

    // MARK: - Keepalive

    private func startKeepAliveLoop() {
        stopKeepAliveLoop()
        keepAliveTask = Task { @MainActor [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: self.keepAlivePingInterval)
                guard !Task.isCancelled else { return }
                self.sendKeepAlivePing()
            }
        }
    }

    private func stopKeepAliveLoop() {
        keepAliveTask?.cancel()
        keepAliveTask = nil
        keepAliveTimeoutTask?.cancel()
        keepAliveTimeoutTask = nil
        awaitingKeepAlivePong = false
    }

    private func sendKeepAlivePing() {
        guard shouldStayConnected, connectionState == .connected, let webSocketTask else { return }
        guard !awaitingKeepAlivePong else {
            handleConnectionFailure(SpacetimeClientConnectionError.keepAliveTimeout)
            return
        }

        awaitingKeepAlivePong = true
        keepAliveTimeoutTask?.cancel()
        keepAliveTimeoutTask = Task { @MainActor [weak self] in
            guard let self else { return }
            try? await Task.sleep(for: self.keepAlivePongTimeout)
            guard self.awaitingKeepAlivePong else { return }
            self.awaitingKeepAlivePong = false
            self.handleConnectionFailure(SpacetimeClientConnectionError.keepAliveTimeout)
        }

        webSocketTask.sendPing { [weak self] error in
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.awaitingKeepAlivePong = false
                self.keepAliveTimeoutTask?.cancel()
                self.keepAliveTimeoutTask = nil
                if let error {
                    self.handleConnectionFailure(error)
                }
            }
        }
    }

    private func nextReconnectDelay() -> Duration? {
        guard let reconnectPolicy else { return nil }
        reconnectAttempt += 1
        return reconnectPolicy.delay(forAttempt: reconnectAttempt)
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
        _ callback: (SpacetimeClientDelegate) -> Void
    ) {
        guard let delegate else { return }
        emitTimedCallbackMetric(named: callbackName) {
            callback(delegate)
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
        self.shouldStayConnected = shouldStayConnected
        handleConnectionFailure(error)
    }

    func _test_deliverServerMessage(_ message: ServerMessage) {
        handleDecodedServerMessage(.success(message))
    }

    func _test_setConnectionState(_ state: ConnectionState) {
        setConnectionState(state)
    }

    func _test_pendingProcedureCallbackCount() -> Int {
        pendingProcedureCallbacks.count
    }

    func _test_pendingOneOffQueryCallbackCount() -> Int {
        pendingOneOffQueryCallbacks.count
    }
}
#endif
