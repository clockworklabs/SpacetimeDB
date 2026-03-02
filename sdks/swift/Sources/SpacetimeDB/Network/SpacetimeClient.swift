import Foundation

public enum SpacetimeClientProcedureError: Error {
    case internalError(String)
    case disconnected
}

@MainActor
public protocol SpacetimeClientDelegate: AnyObject {
    func onConnect()
    func onDisconnect(error: Error?)
    func onIdentityReceived(identity: [UInt8], token: String)
    func onTransactionUpdate(message: Data?)
    func onReducerError(reducer: String, message: String, isInternal: Bool)
}

public extension SpacetimeClientDelegate {
    func onReducerError(reducer: String, message: String, isInternal: Bool) {}
}

@MainActor
public final class SpacetimeClient: @unchecked Sendable {
    public let serverUrl: URL
    public let moduleName: String
    public weak var delegate: SpacetimeClientDelegate?

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

    public init(
        serverUrl: URL,
        moduleName: String,
        reconnectPolicy: ReconnectPolicy? = ReconnectPolicy(),
        compressionMode: CompressionMode = .gzip
    ) {
        self.serverUrl = serverUrl
        self.moduleName = moduleName
        self.reconnectPolicy = reconnectPolicy
        self.compressionMode = compressionMode
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
        startNetworkMonitor()
        performConnect(authToken: token ?? self.savedToken)
    }

    private func performConnect(authToken: String?) {
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
        webSocketTask = urlSession.webSocketTask(with: request)
        webSocketTask?.resume()
        receiveMessage()
    }

    public func disconnect() {
        shouldStayConnected = false
        stopNetworkMonitor()
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
        delegate?.onDisconnect(error: nil)
    }

    // MARK: - Serialized send queue

    private func enqueue(_ message: URLSessionWebSocketTask.Message) {
        sendQueue.append(message)
        flushQueue()
    }

    private func flushQueue() {
        guard !isSending, !sendQueue.isEmpty, let task = webSocketTask else { return }
        let msg = sendQueue.removeFirst()
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
        pendingProcedureCallbacks[requestId] = { result in
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
        try await withCheckedThrowingContinuation { continuation in
            sendProcedure(procedureName, args) { result in
                switch result {
                case .success(let value):
                    continuation.resume(returning: value)
                case .failure(let error):
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    public func sendProcedure<R: Decodable & Sendable>(
        _ procedureName: String,
        _ args: Data,
        responseType: R.Type
    ) async throws -> R {
        try await withCheckedThrowingContinuation { continuation in
            sendProcedure(procedureName, args, responseType: responseType) { result in
                switch result {
                case .success(let value):
                    continuation.resume(returning: value)
                case .failure(let error):
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    public func oneOffQuery(_ query: String, completion: @escaping (Result<QueryRows, Error>) -> Void) {
        let requestId = allocateRequestId()
        pendingOneOffQueryCallbacks[requestId] = completion
        send(ClientMessage.oneOffQuery(OneOffQuery(requestId: requestId, queryString: query)))
    }

    public func oneOffQuery(_ query: String) async throws -> QueryRows {
        try await withCheckedThrowingContinuation { continuation in
            oneOffQuery(query) { result in
                switch result {
                case .success(let rows):
                    continuation.resume(returning: rows)
                case .failure(let error):
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    public func subscribe(
        queries: [String],
        onApplied: (() -> Void)? = nil,
        onError: ((String) -> Void)? = nil
    ) -> SubscriptionHandle {
        let handle = SubscriptionHandle(queries: queries, client: self, onApplied: onApplied, onError: onError)
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
                    guard self.shouldStayConnected else { return }
                    Log.network.error("WebSocket error: \(error.localizedDescription)")
                    self.failPendingProcedureCallbacks(with: error)
                    self.failPendingOneOffQueryCallbacks(with: error)
                    self.delegate?.onDisconnect(error: error)

                    guard let reconnectDelay = self.nextReconnectDelay() else {
                        return
                    }

                    // If network is unavailable, defer reconnection until path restores
                    guard self.networkMonitor?.isConnected ?? true else {
                        Log.network.info("Network unavailable, deferring reconnect until path restores")
                        return
                    }

                    try? await Task.sleep(for: reconnectDelay)
                    guard self.shouldStayConnected else { return }
                    self.performConnect(authToken: self.savedToken)
                case .success(let message):
                    switch message {
                    case .data(let data):
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
                delegate?.onIdentityReceived(identity: Array(connection.identity), token: connection.token)
                subscribeAll(tables: Array(Self.clientCache.registeredTableNames))
                resubscribeManagedSubscriptions()
                delegate?.onConnect()
            case .transactionUpdate(let update):
                Self.clientCache.applyTransactionUpdate(update)
                delegate?.onTransactionUpdate(message: nil)
            case .subscribeApplied(let applied):
                handleSubscribeApplied(applied)
                let initial = applied.asTransactionUpdate()
                Self.clientCache.applyTransactionUpdate(initial)
                delegate?.onTransactionUpdate(message: nil)
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
            delegate?.onReducerError(reducer: reducerName, message: message, isInternal: false)
        case .internalError(let message):
            Log.client.error("Reducer request_id=\(reducerResult.requestId) internal error: \(message)")
            delegate?.onReducerError(reducer: reducerName, message: message, isInternal: true)
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
            delegate?.onTransactionUpdate(message: nil)
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
                self.performConnect(authToken: self.savedToken)
            }
        }
        monitor.start()
        networkMonitor = monitor
    }

    private func stopNetworkMonitor() {
        networkMonitor?.stop()
        networkMonitor = nil
    }

    private func nextReconnectDelay() -> Duration? {
        guard let reconnectPolicy else { return nil }
        reconnectAttempt += 1
        return reconnectPolicy.delay(forAttempt: reconnectAttempt)
    }
}
