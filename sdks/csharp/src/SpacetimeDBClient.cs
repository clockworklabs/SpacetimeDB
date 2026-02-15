using System;
using System.Collections;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Thread = System.Threading.Thread;

namespace SpacetimeDB
{
    public sealed class DbConnectionBuilder<DbConnection>
        where DbConnection : IDbConnection, new()
    {
        readonly DbConnection conn = new();

        string? uri;
        string? nameOrAddress;
        string? token;
        Compression? compression;
        bool light;
        bool? confirmedReads;

        public DbConnection Build()
        {
            if (uri == null)
            {
                throw new InvalidOperationException("Building DbConnection with a null uri. Call WithUri() first.");
            }
            if (nameOrAddress == null)
            {
                throw new InvalidOperationException("Building DbConnection with a null nameOrAddress. Call WithDatabaseName() first.");
            }
            conn.Connect(token, uri, nameOrAddress, compression ?? Compression.Brotli, light, confirmedReads);
#if UNITY_5_3_OR_NEWER
            if (SpacetimeDBNetworkManager._instance != null)
            {
                SpacetimeDBNetworkManager._instance.AddConnection(conn);
            }
#endif
            return conn;
        }

        public DbConnectionBuilder<DbConnection> WithUri(string uri)
        {
            this.uri = uri;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithDatabaseName(string nameOrAddress)
        {
            this.nameOrAddress = nameOrAddress;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithToken(string? token)
        {
            this.token = token;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithCompression(Compression compression)
        {
            this.compression = compression;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithLightMode(bool light)
        {
            this.light = light;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithConfirmedReads(bool confirmedReads)
        {
            this.confirmedReads = confirmedReads;
            return this;
        }

        public delegate void ConnectCallback(DbConnection conn, Identity identity, string token);

        public DbConnectionBuilder<DbConnection> OnConnect(ConnectCallback cb)
        {
            conn.AddOnConnect((identity, token) => cb(conn, identity, token));
            return this;
        }

        public delegate void ConnectErrorCallback(Exception e);

        public DbConnectionBuilder<DbConnection> OnConnectError(ConnectErrorCallback cb)
        {
            conn.AddOnConnectError(e => cb(e));
            return this;
        }

        public delegate void DisconnectCallback(DbConnection conn, Exception? e);

        public DbConnectionBuilder<DbConnection> OnDisconnect(DisconnectCallback cb)
        {
            conn.AddOnDisconnect(e => cb(conn, e));
            return this;
        }
    }

    public interface IDbConnection
    {
        internal void Connect(string? token, string uri, string addressOrName, Compression compression, bool light, bool? confirmedReads);

        internal void AddOnConnect(Action<Identity, string> cb);
        internal void AddOnConnectError(WebSocket.ConnectErrorEventHandler cb);
        internal void AddOnDisconnect(WebSocket.CloseEventHandler cb);

        internal QuerySetId? Subscribe(ISubscriptionHandle handle, string[] querySqls);
        internal void Unsubscribe(QuerySetId queryId);
        void FrameTick();
        void Disconnect();

        internal Task<T[]> RemoteQuery<T>(string query) where T : IStructuralReadWrite, new();
        void InternalCallReducer<T>(T args)
            where T : IReducerArgs, new();

        void InternalCallProcedure<TArgs, TReturn>(
            TArgs args,
            ProcedureCallback<TReturn> callback)
            where TArgs : IProcedureArgs, new()
            where TReturn : IStructuralReadWrite, new();
    }

    public abstract class DbConnectionBase<DbConnection, Tables, Reducer> : IDbConnection
        where DbConnection : DbConnectionBase<DbConnection, Tables, Reducer>, new()
        where Tables : RemoteTablesBase
    {
        public static DbConnectionBuilder<DbConnection> Builder() => new();

        internal event Action<Identity, string>? onConnect;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        [Obsolete]
        public event Action<Exception>? onSendError;

        /// <summary>
        /// Dictionary of subscriptions, keyed by query ID.
        /// </summary>
        private readonly Dictionary<uint, ISubscriptionHandle> subscriptions = new();

        /// <summary>
        /// Allocates query IDs.
        /// </summary>
        private UintAllocator querySetIdAllocator;

        public readonly ConnectionId ConnectionId = ConnectionId.Random();
        public Identity? Identity { get; private set; }
        private ConnectionId? initialConnectionId;
        private bool onConnectInvoked;

        internal WebSocket webSocket;
        private bool connectionClosed;
        public abstract Tables Db { get; }

        protected abstract IEventContext ToEventContext(Event<Reducer> Event);
        protected abstract IReducerEventContext ToReducerEventContext(ReducerEvent<Reducer> reducerEvent);
        protected abstract ISubscriptionEventContext MakeSubscriptionEventContext();
        protected abstract IErrorContext ToErrorContext(Exception errorContext);
        protected abstract IProcedureEventContext ToProcedureEventContext(ProcedureEvent procedureEvent);

        private readonly ConcurrentDictionary<uint, TaskCompletionSource<OneOffQueryResult>> waitingOneOffQueries = new();

        private readonly ConcurrentDictionary<uint, PendingReducerCall> pendingReducerCalls = new();

        private sealed class PendingReducerCall
        {
            public Reducer Reducer = default!;
        }

        private readonly ProcedureCallbacks procedureCallbacks = new();

        private void FailPendingOperations(Exception error)
        {
            foreach (var (requestId, _) in waitingOneOffQueries.ToArray())
            {
                if (waitingOneOffQueries.TryRemove(requestId, out var resultSource))
                {
                    resultSource.TrySetException(error);
                }
            }

            pendingReducerCalls.Clear();

            try
            {
                var procedureEvent = new ProcedureEvent(
                    default,
                    new ProcedureStatus.InternalError(error.Message),
                    Identity ?? default,
                    ConnectionId,
                    default,
                    0
                );
                var ctx = ToProcedureEventContext(procedureEvent);
                procedureCallbacks.FailAll(ctx, error);
            }
            catch
            {
                // If we cannot construct a procedure context, still avoid retaining stale callbacks.
                procedureCallbacks.Clear();
            }
        }

        private bool isClosing;
        private readonly Thread networkMessageParseThread;
        public readonly Stats stats = new();

        protected DbConnectionBase()
        {
            var options = new WebSocket.ConnectOptions
            {
                Protocol = "v2.bsatn.spacetimedb"
            };
            webSocket = new WebSocket(options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnSendError += a => onSendError?.Invoke(a);
#if UNITY_5_3_OR_NEWER
            webSocket.OnClose += (e) =>
            {
                if (SpacetimeDBNetworkManager._instance != null)
                {
                    SpacetimeDBNetworkManager._instance.RemoveConnection(this);
                }
            };

#if UNITY_WEBGL && !UNITY_EDITOR
            if (SpacetimeDBNetworkManager._instance != null)
                SpacetimeDBNetworkManager._instance.StartCoroutine(ParseMessages());
#endif
#endif

#if !(UNITY_WEBGL && !UNITY_EDITOR)
            // For targets other than webgl we start a thread to parse messages
            networkMessageParseThread = new Thread(ParseMessages);
            networkMessageParseThread.Name = "SpacetimeDB Network Thread";
            networkMessageParseThread.Start();
#endif
        }

        internal struct UnparsedMessage
        {
            /// <summary>
            /// The bytes of the message.
            /// </summary>
            public byte[] bytes;

            /// <summary>
            /// The timestamp the message came off the wire.
            /// </summary>
            public DateTime timestamp;

            /// <summary>
            /// The ID assigned by the message parsing queue tracker.
            /// </summary>
            public uint parseQueueTrackerId;
        }

        internal struct ParsedMessage
        {
            public ServerMessage message;
            public ParsedDatabaseUpdate dbOps;
            public DateTime receiveTimestamp;
            public uint applyQueueTrackerId;
            public ReducerEvent<Reducer>? reducerEvent;
            public ProcedureEvent? procedureEvent;
        }

        private readonly BlockingCollection<UnparsedMessage> _parseQueue =
            new(new ConcurrentQueue<UnparsedMessage>());

        private readonly BlockingCollection<ParsedMessage> _applyQueue =
            new(new ConcurrentQueue<ParsedMessage>());

        internal static bool IsTesting;
        internal bool HasMessageToApply => _applyQueue.Count > 0;

        private readonly CancellationTokenSource _parseCancellationTokenSource = new();
        private CancellationToken _parseCancellationToken => _parseCancellationTokenSource.Token;

        private static readonly Status Committed = new Status.Committed(default);

        /// <summary>
        /// Get a description of a message suitable for storing in the tracker metadata.
        /// </summary>
        /// <param name="message"></param>
        /// <returns></returns>
        internal string TrackerMetadataForMessage(ServerMessage message) => message switch
        {
            ServerMessage.TransactionUpdate(var transactionUpdate) => $"type={nameof(ServerMessage.TransactionUpdate)},query_sets={transactionUpdate.QuerySets.Count}",
            ServerMessage.ReducerResult(var reducerResult) => $"type={nameof(ServerMessage.ReducerResult)},request_id={reducerResult.RequestId}",
            _ => $"type={message.GetType().Name}"
        };

#if UNITY_WEBGL && !UNITY_EDITOR
        internal IEnumerator ParseMessages()
#else
        internal void ParseMessages()
#endif
        {
            static BsatnRowList EmptyRowList() => new(new RowSizeHint.RowOffsets(new()), new());

            IEnumerable<(IRemoteTableHandle, TableUpdate)> GetTables(IEnumerable<TableUpdate> updates)
            {
                foreach (var update in updates)
                {
                    var tableName = update.TableName;
                    var table = Db.GetTable(tableName);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableName}");
                        continue;
                    }
                    yield return (table, update);
                }
            }

            ParsedDatabaseUpdate ParseSubscribeRows(QueryRows queryRows)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                var empty = EmptyRowList();
                foreach (var tableRows in queryRows.Tables)
                {
                    var table = Db.GetTable(tableRows.Table);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableRows.Table}");
                        continue;
                    }

                    var update = new TableUpdate
                    {
                        TableName = tableRows.Table,
                        Rows = new List<TableUpdateRows>
                        {
                            new TableUpdateRows.PersistentTable(
                                new PersistentTableRows(tableRows.Rows, empty)
                            )
                        }
                    };
                    table.ParseInsertOnly(update, dbOps);
                }
                return dbOps;
            }

            ParsedDatabaseUpdate ParseUnsubscribeRows(QueryRows queryRows)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                var empty = EmptyRowList();
                foreach (var tableRows in queryRows.Tables)
                {
                    var table = Db.GetTable(tableRows.Table);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableRows.Table}");
                        continue;
                    }

                    var update = new TableUpdate
                    {
                        TableName = tableRows.Table,
                        Rows = new List<TableUpdateRows>
                        {
                            new TableUpdateRows.PersistentTable(
                                new PersistentTableRows(empty, tableRows.Rows)
                            )
                        }
                    };
                    table.ParseDeleteOnly(update, dbOps);
                }
                return dbOps;
            }

            ParsedDatabaseUpdate ParseTransactionUpdate(TransactionUpdate update)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                foreach (var set in update.QuerySets)
                {
                    foreach (var (table, tableUpdate) in GetTables(set.Tables))
                    {
                        table.Parse(tableUpdate, dbOps);
                    }
                }
                return dbOps;
            }

            string DecodeReducerError(IReadOnlyList<byte> bytes)
            {
                try
                {
                    using var stream = new MemoryStream(bytes.ToArray());
                    using var reader = new BinaryReader(stream);
                    return new SpacetimeDB.BSATN.String().Read(reader);
                }
                catch
                {
                    return $"Reducer returned undecodable BSATN string bytes (len={bytes.Count})";
                }
            }

            void ParseOneOffQuery(OneOffQueryResult resp)
            {
                if (!waitingOneOffQueries.TryRemove(resp.RequestId, out var resultSource))
                {
                    Log.Error($"Response to unknown one-off-query request_id: {resp.RequestId}");
                    return;
                }

                resultSource.TrySetResult(resp);
            }

            while (!isClosing)
            {

#if UNITY_WEBGL && !UNITY_EDITOR
                yield return null;
                while (_parseQueue.Count > 0)
#endif
                try
                {
                    var message = _parseQueue.Take(_parseCancellationToken);
                    var parsedMessage = ParseMessage(message);
                    _applyQueue.Add(parsedMessage, _parseCancellationToken);
                }
                catch (OperationCanceledException)
                {
#if UNITY_WEBGL && !UNITY_EDITOR
                    break;
#else
                    return; // Normal shutdown
#endif
                }
                catch (Exception e)
                {
                    Log.Exception(e);
                    FailPendingOperations(new OperationCanceledException("Message parsing failed; connection closed.", e));
                    Disconnect();
#if UNITY_WEBGL && !UNITY_EDITOR
                    break;
#else
                    return;
#endif
                }
            }

            ParsedMessage ParseMessage(UnparsedMessage unparsed)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                var message = CompressionHelpers.DecompressDecodeMessage(unparsed.bytes);
                var trackerMetadata = TrackerMetadataForMessage(message);

                stats.ParseMessageQueueTracker.FinishTrackingRequest(unparsed.parseQueueTrackerId, trackerMetadata);
                var parseStart = DateTime.UtcNow;

                ReducerEvent<Reducer>? reducerEvent = default;
                ProcedureEvent? procedureEvent = default;

                switch (message)
                {
                    case ServerMessage.InitialConnection:
                        break;
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(subscribeApplied.RequestId, unparsed.timestamp);
                        dbOps = ParseSubscribeRows(subscribeApplied.Rows);
                        break;
                    case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(unsubscribeApplied.RequestId, unparsed.timestamp);
                        if (unsubscribeApplied.Rows != null)
                        {
                            dbOps = ParseUnsubscribeRows(unsubscribeApplied.Rows);
                        }
                        break;
                    case ServerMessage.SubscriptionError(var subscriptionError):
                        if (subscriptionError.RequestId.HasValue)
                        {
                            stats.SubscriptionRequestTracker.FinishTrackingRequest(subscriptionError.RequestId.Value, unparsed.timestamp);
                        }
                        break;
                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        dbOps = ParseTransactionUpdate(transactionUpdate);
                        break;
                    case ServerMessage.OneOffQueryResult(var resp):
                        ParseOneOffQuery(resp);
                        break;
                    case ServerMessage.ReducerResult(var reducerResult):
                        if (!stats.ReducerRequestTracker.FinishTrackingRequest(reducerResult.RequestId, unparsed.timestamp))
                        {
                            Log.Warn($"Failed to finish tracking reducer request: {reducerResult.RequestId}");
                        }

                        var reducerStatus = reducerResult.Result switch
                        {
                            ReducerOutcome.Ok => Committed,
                            ReducerOutcome.Okmpty => Committed,
                            ReducerOutcome.Err(var err) => new Status.Failed(DecodeReducerError(err)),
                            ReducerOutcome.InternalError(var err) => new Status.Failed(err),
                            _ => new Status.Failed("Unknown reducer result"),
                        };

                        if (reducerResult.Result is ReducerOutcome.Ok(var ok))
                        {
                            dbOps = ParseTransactionUpdate(ok.TransactionUpdate);
                        }

                        if (pendingReducerCalls.TryRemove(reducerResult.RequestId, out var pendingReducer))
                        {
                            try
                            {
                                reducerEvent = new(
                                    (DateTimeOffset)reducerResult.Timestamp,
                                    reducerStatus,
                                    Identity ?? throw new InvalidOperationException("Identity not set"),
                                    ConnectionId,
                                    null,
                                    pendingReducer.Reducer);
                            }
                            catch (Exception)
                            {
                                // The local reducer request still completed; failure here should not block update apply.
                            }
                        }
                        else
                        {
                            throw new InvalidOperationException(
                                $"Reducer result for unknown request_id {reducerResult.RequestId}"
                            );
                        }
                        break;
                    case ServerMessage.ProcedureResult(var procedureResult):
                        procedureEvent = new ProcedureEvent(
                            procedureResult.Timestamp,
                            procedureResult.Status,
                            Identity ?? throw new InvalidOperationException("Identity not set"),
                            ConnectionId,
                            procedureResult.TotalHostExecutionDuration,
                            procedureResult.RequestId
                        );

                        if (!stats.ProcedureRequestTracker.FinishTrackingRequest(procedureResult.RequestId, unparsed.timestamp))
                        {
                            Log.Warn($"Failed to finish tracking procedure request: {procedureResult.RequestId}");
                        }

                        break;
                    default:
                        throw new InvalidOperationException();
                }

                stats.ParseMessageTracker.InsertRequest(parseStart, trackerMetadata);
                var applyTracker = stats.ApplyMessageQueueTracker.StartTrackingRequest(trackerMetadata);

                return new ParsedMessage { message = message, dbOps = dbOps, receiveTimestamp = unparsed.timestamp, applyQueueTrackerId = applyTracker, reducerEvent = reducerEvent, procedureEvent = procedureEvent };
            }
        }

        public void Disconnect()
        {
            isClosing = true;
            connectionClosed = true;
            FailPendingOperations(new OperationCanceledException("Connection closed."));

            // Only try to close if the connection is active
            if (webSocket.IsConnected)
            {
                webSocket.Close();
            }
#if UNITY_WEBGL && !UNITY_EDITOR
            else if (webSocket.IsConnecting)
#else
            else if (webSocket.IsConnecting || webSocket.IsNoneState)
#endif
            {
                webSocket.Abort(); // forceful during connecting
            }

            _parseCancellationTokenSource.Cancel();
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="uri"> URI of the SpacetimeDB server (ex: https://maincloud.spacetimedb.com)
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        /// <param name="compression">The compression settings to use</param>
        /// <param name="light">Whether or not to request light updates</param>
        /// <param name="confirmedReads">
        /// If set to true, instruct the server to send updates for transactions
        /// only after they are confirmed to be durable.
        ///
        /// What durable means depends on the server configuration. In general,
        /// a transaction is durable when it has been written to disk on one or
        /// more servers.
        ///
        /// If set to false, instruct the server to send updates as soon as
        /// transactions are committed in memory.
        ///
        /// If not set, the server chooses the default.
        /// </param>
        void IDbConnection.Connect(string? token, string uri, string addressOrName, Compression compression, bool light, bool? confirmedReads)
        {
            isClosing = false;
            connectionClosed = false;
            Identity = null;
            initialConnectionId = null;
            onConnectInvoked = false;
            while (_parseQueue.TryTake(out _)) { }
            while (_applyQueue.TryTake(out _)) { }

            uri = uri.Replace("http://", "ws://");
            uri = uri.Replace("https://", "wss://");
            if (!uri.StartsWith("ws://") && !uri.StartsWith("wss://"))
            {
                uri = $"ws://{uri}";
            }
            // Things fail surprisingly if we have a trailing slash, because we later manually append strings
            // like `/foo` and then end up with `//` in the URI.
            uri = uri.TrimEnd('/');

            Log.Info($"SpacetimeDBClient: Connecting to {uri} {addressOrName}");
            if (!IsTesting)
            {
#if UNITY_WEBGL && !UNITY_EDITOR
                async Task Function()
#else
                Task.Run(async () =>
#endif
                {
                    try
                    {
                        await webSocket.Connect(token, uri, addressOrName, ConnectionId, compression, light, confirmedReads);
                    }
                    catch (Exception e)
                    {
                        if (connectionClosed)
                        {
                            Log.Info("Connection closed gracefully.");
                            return;
                        }

                        Log.Exception(e);
                    }
#if UNITY_WEBGL && !UNITY_EDITOR
                }
                _ = Function();
#else
                });
#endif
            }
        }


        private void ApplyUpdate(IEventContext eventContext, ParsedDatabaseUpdate dbOps)
        {
            // First trigger OnBeforeDelete
            foreach (var (table, update) in dbOps.Updates)
            {
                table.PreApply(eventContext, update);
            }

            foreach (var (table, update) in dbOps.Updates)
            {
                table.Apply(eventContext, update);
            }

            foreach (var (table, _) in dbOps.Updates)
            {
                table.PostApply(eventContext);
            }
        }

        protected abstract bool Dispatch(IReducerEventContext context, Reducer reducer);

        private void ApplyMessage(ParsedMessage parsed)
        {
            var message = parsed.message;
            var dbOps = parsed.dbOps;
            var timestamp = parsed.receiveTimestamp;

            stats.ApplyMessageQueueTracker.FinishTrackingRequest(parsed.applyQueueTrackerId);
            var applyStart = DateTime.UtcNow;

            switch (message)
            {
                case ServerMessage.SubscribeApplied(var subscribeApplied):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(subscribeApplied.QuerySetId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnApplied(eventContext);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }
                        else
                        {
                            Log.Warn($"Received SubscribeApplied for unknown query_set_id={subscribeApplied.QuerySetId.Id}");
                        }

                        break;
                    }

                case ServerMessage.SubscriptionError(var subscriptionError):
                    {
                        Log.Warn($"Subscription Error: ${subscriptionError.Error}");

                        // TODO: should I use a more specific exception type here?
                        var exception = new Exception(subscriptionError.Error);
                        var eventContext = ToErrorContext(exception);
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeError(exception));
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(subscriptionError.QuerySetId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnError(eventContext);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }
                        else
                        {
                            Log.Warn($"Received SubscriptionError for unknown query_set_id={subscriptionError.QuerySetId.Id}");
                        }

                        break;
                    }

                case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.UnsubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(unsubscribeApplied.QuerySetId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnEnded(eventContext);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }

                        subscriptions.Remove(unsubscribeApplied.QuerySetId.Id);
                    }
                    break;

                case ServerMessage.TransactionUpdate(var transactionUpdate):
                    {
                        var legacyEventContext = ToEventContext(new Event<Reducer>.Transaction());
                        ApplyUpdate(legacyEventContext, dbOps);
                        break;
                    }
                case ServerMessage.ReducerResult(var reducerResult):
                    {
                        if (parsed.reducerEvent is { } reducerEvent)
                        {
                            var legacyEventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                            ApplyUpdate(legacyEventContext, dbOps);
                            var eventContext = ToReducerEventContext(reducerEvent);
                            Dispatch(eventContext, reducerEvent.Reducer);
                        }
                        else
                        {
                            var legacyEventContext = ToEventContext(new Event<Reducer>.UnknownTransaction());
                            ApplyUpdate(legacyEventContext, dbOps);
                        }
                        break;
                    }
                case ServerMessage.InitialConnection(var initialConnection):
                    try
                    {
                        if (Identity is Identity identity && identity != initialConnection.Identity)
                        {
                            throw new InvalidOperationException(
                                $"Received InitialConnection with unexpected identity. Previous={identity}, New={initialConnection.Identity}"
                            );
                        }

                        if (initialConnectionId is ConnectionId connectionId
                            && connectionId != initialConnection.ConnectionId)
                        {
                            throw new InvalidOperationException(
                                $"Received InitialConnection with unexpected connection_id. Previous={connectionId}, New={initialConnection.ConnectionId}"
                            );
                        }

                        Identity = initialConnection.Identity;
                        initialConnectionId = initialConnection.ConnectionId;
                        if (!onConnectInvoked)
                        {
                            onConnectInvoked = true;
                            onConnect?.Invoke(initialConnection.Identity, initialConnection.Token);
                            onConnect = null;
                        }
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                    break;

                case ServerMessage.OneOffQueryResult:
                    /* OneOffQuery is async and handles its own responses */
                    break;
                case ServerMessage.ProcedureResult(var procedureResult):
                    var procedureEventContext = ToProcedureEventContext(parsed.procedureEvent!);
                    if (!procedureCallbacks.TryResolveCallback(procedureEventContext, procedureResult.RequestId, procedureResult))
                    {
                        Log.Warn($"Received ProcedureResult for unknown request ID: {procedureResult.RequestId}");
                    }
                    break;
                default:
                    throw new InvalidOperationException();
            }

            stats.ApplyMessageTracker.InsertRequest(applyStart, TrackerMetadataForMessage(message));
        }

        // Note: this method is called from unit tests.
        internal void OnMessageReceived(byte[] bytes, DateTime timestamp)
        {
            _parseQueue.Add(new UnparsedMessage { bytes = bytes, timestamp = timestamp, parseQueueTrackerId = stats.ParseMessageQueueTracker.StartTrackingRequest() });
        }

        void IDbConnection.InternalCallReducer<T>(T args)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call reducer, not connected to server!");
                return;
            }

            var requestId = stats.ReducerRequestTracker.StartTrackingRequest(args.ReducerName);
            if (args is not Reducer typedReducer)
            {
                throw new InvalidOperationException(
                    $"Reducer arguments type {typeof(T).FullName} is not assignable to {typeof(Reducer).FullName}."
                );
            }

            var encodedArgs = IStructuralReadWrite.ToBytes(args).ToList();
            var pendingReducer = new PendingReducerCall
            {
                Reducer = typedReducer,
            };
            pendingReducerCalls[requestId] = pendingReducer;
            webSocket.Send(new ClientMessage.CallReducer(new CallReducer(
                requestId,
                0, // v2 parity with Rust SDK: always CallReducerFlags::Default.
                args.ReducerName,
                encodedArgs
            )));
        }

        // TODO: Replace with an internal interface 
        void IDbConnection.InternalCallProcedure<TArgs, TReturn>(
            TArgs args,
            ProcedureCallback<TReturn> callback)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call procedure, not connected to server!");
                return;
            }

            var requestId = stats.ProcedureRequestTracker.StartTrackingRequest(args.ProcedureName);
            procedureCallbacks.RegisterCallback(requestId, callback);

            webSocket.Send(new ClientMessage.CallProcedure(new CallProcedure(
                requestId,
                0,
                args.ProcedureName,
                IStructuralReadWrite.ToBytes(args).ToList()
            )));
        }

        QuerySetId? IDbConnection.Subscribe(ISubscriptionHandle handle, string[] querySqls)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return null;
            }

            var id = stats.SubscriptionRequestTracker.StartTrackingRequest();
            // We use a distinct ID from the request ID as a sanity check that we're not
            // casting request IDs to query IDs anywhere in the new code path.
            var querySetId = querySetIdAllocator.Next();
            subscriptions[querySetId] = handle;
            webSocket.Send(new ClientMessage.Subscribe(
                new Subscribe
                {
                    RequestId = id,
                    QuerySetId = new QuerySetId(querySetId),
                    QueryStrings = querySqls.ToList(),
                }
            ));
            return new QuerySetId(querySetId);
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("SELECT * FROM table WHERE sender = \"bob\"");
        [Obsolete("This is replaced by ctx.Db.TableName.RemoteQuery(\"WHERE ...\")", false)]
        public Task<T[]> OneOffQuery<T>(string query) where T : IStructuralReadWrite, new() =>
            ((IDbConnection)this).RemoteQuery<T>(query);

        async Task<T[]> IDbConnection.RemoteQuery<T>(string query)
        {
            var requestId = stats.OneOffRequestTracker.StartTrackingRequest();
            var resultSource = new TaskCompletionSource<OneOffQueryResult>();
            waitingOneOffQueries[requestId] = resultSource;

            webSocket.Send(new ClientMessage.OneOffQuery(new OneOffQuery
            {
                RequestId = requestId,
                QueryString = query,
            }));

            var result = await resultSource.Task;

            if (!stats.OneOffRequestTracker.FinishTrackingRequest(requestId))
            {
                Log.Warn($"Failed to finish tracking one off request: {requestId}");
            }

            T[] LogAndThrow(string error)
            {
                error = $"While processing one-off-query `{query}`, request_id {requestId}: {error}";
                Log.Error(error);
                throw new Exception(error);
            }

            if (result.Result is Result<QueryRows, string>.ErrR(var err))
            {
                return LogAndThrow($"Server error: {err}");
            }

            if (result.Result is not Result<QueryRows, string>.OkR(var rows))
            {
                return LogAndThrow("Unexpected one-off query result variant");
            }

            var tables = rows.Tables;
            if (tables.Count != 1)
            {
                return LogAndThrow($"Expected a single table, but got {tables.Count}");
            }

            var resultTable = tables[0];
            var cacheTable = Db.GetTable(resultTable.Table);

            if (cacheTable?.ClientTableType != typeof(T))
            {
                return LogAndThrow($"Mismatched result type, expected {typeof(T)} but got {resultTable.Table}");
            }

            var (resultReader, resultCount) = CompressionHelpers.ParseRowList(resultTable.Rows);
            var output = new T[resultCount];
            for (int i = 0; i < resultCount; i++)
            {
                output[i] = IStructuralReadWrite.Read<T>(resultReader);
            }
            return output;
        }

        public bool IsActive => webSocket.IsConnected;

        public void FrameTick()
        {
            webSocket.Update();
            while (_applyQueue.TryTake(out var parsedMessage))
            {
                ApplyMessage(parsedMessage);
            }
        }

        void IDbConnection.Unsubscribe(QuerySetId queryId)
        {
            if (!subscriptions.ContainsKey(queryId.Id))
            {
                Log.Warn($"Unsubscribing from a subscription that the DbConnection does not know about, with QuerySetId {queryId.Id}");
            }

            var requestId = stats.SubscriptionRequestTracker.StartTrackingRequest();

            webSocket.Send(new ClientMessage.Unsubscribe(new()
            {
                RequestId = requestId,
                QuerySetId = queryId,
                Flags = UnsubscribeFlags.SendDroppedRows,
            }));

        }

        void IDbConnection.AddOnConnect(Action<Identity, string> cb) => onConnect += cb;

        void IDbConnection.AddOnConnectError(WebSocket.ConnectErrorEventHandler cb) => webSocket.OnConnectError += cb;

        void IDbConnection.AddOnDisconnect(WebSocket.CloseEventHandler cb) => webSocket.OnClose += cb;
    }

    /// <summary>
    /// Represents the result of parsing a database update message from SpacetimeDB.
    /// Contains updates for all tables affected by the update, with each entry mapping a table handle
    /// to its respective set of row changes (by primary key or row instance).
    ///
    /// Note: Due to C#'s struct constructor limitations, you must use <see cref="ParsedDatabaseUpdate.New"/>
    /// to create new instances.
    /// Do not use the default constructor, as it will not initialize the Updates dictionary.
    /// </summary>
    internal struct ParsedDatabaseUpdate
    {
        // Map: table handles -> (primary key -> IStructuralReadWrite).
        // If a particular table has no primary key, the "primary key" is just the row itself.
        // This is valid because any [SpacetimeDB.Type] automatically has a correct Equals and HashSet implementation.
        public Dictionary<IRemoteTableHandle, IParsedTableUpdate> Updates;

        // Can't override the default constructor. Make sure you use this one!
        public static ParsedDatabaseUpdate New()
        {
            ParsedDatabaseUpdate result;
            result.Updates = new();
            return result;
        }

        /// <summary>
        /// Returns the <see cref="IParsedTableUpdate"/> for the specified table.
        /// If no update exists for the table, a new one is allocated and added to the Updates dictionary.
        /// </summary>
        public IParsedTableUpdate UpdateForTable(IRemoteTableHandle table)
        {
            if (!Updates.TryGetValue(table, out var delta))
            {
                delta = table.MakeParsedTableUpdate();
                Updates[table] = delta;
            }

            return delta;
        }
    }

    internal struct UintAllocator
    {
        private uint lastAllocated;

        /// <summary>
        /// Allocate a new ID in a thread-unsafe way.
        /// </summary>
        /// <returns>A previously-unused ID.</returns>
        public uint Next()
        {
            lastAllocated++;
            return lastAllocated;
        }
    }
}
