using System;
using System.Collections;
using System.Collections.Concurrent;
using System.Collections.Generic;
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
                throw new InvalidOperationException("Building DbConnection with a null nameOrAddress. Call WithModuleName() first.");
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

        public DbConnectionBuilder<DbConnection> WithModuleName(string nameOrAddress)
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

        internal void LegacySubscribe(ISubscriptionHandle handle, string[] querySqls);
        internal QueryId? Subscribe(ISubscriptionHandle handle, string[] querySqls);
        internal void Unsubscribe(QueryId queryId);
        void FrameTick();
        void Disconnect();

        internal Task<T[]> RemoteQuery<T>(string query) where T : IStructuralReadWrite, new();
        void InternalCallReducer<T>(T args, CallReducerFlags flags)
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
        /// Dictionary of legacy subscriptions, keyed by request ID rather than query ID.
        /// Only used for `SubscribeToAllTables()`.
        /// </summary>
        private readonly Dictionary<uint, ISubscriptionHandle> legacySubscriptions = new();

        /// <summary>
        /// Dictionary of subscriptions, keyed by query ID.
        /// </summary>
        private readonly Dictionary<uint, ISubscriptionHandle> subscriptions = new();

        /// <summary>
        /// Allocates query IDs.
        /// </summary>
        private UintAllocator queryIdAllocator;

        public readonly ConnectionId ConnectionId = ConnectionId.Random();
        public Identity? Identity { get; private set; }

        internal WebSocket webSocket;
        private bool connectionClosed;
        public abstract Tables Db { get; }

        protected abstract Reducer ToReducer(TransactionUpdate update);
        protected abstract IEventContext ToEventContext(Event<Reducer> Event);
        protected abstract IReducerEventContext ToReducerEventContext(ReducerEvent<Reducer> reducerEvent);
        protected abstract ISubscriptionEventContext MakeSubscriptionEventContext();
        protected abstract IErrorContext ToErrorContext(Exception errorContext);
        protected abstract IProcedureEventContext ToProcedureEventContext(ProcedureEvent procedureEvent);

        private readonly Dictionary<Guid, TaskCompletionSource<OneOffQueryResponse>> waitingOneOffQueries = new();

        private readonly ProcedureCallbacks procedureCallbacks = new();

        private bool isClosing;
        private readonly Thread networkMessageParseThread;
        public readonly Stats stats = new();

        protected DbConnectionBase()
        {
            var options = new WebSocket.ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bsatn.spacetimedb"
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
        private static readonly Status OutOfEnergy = new Status.OutOfEnergy(default);

        /// <summary>
        /// Get a description of a message suitable for storing in the tracker metadata.
        /// </summary>
        /// <param name="message"></param>
        /// <returns></returns>
        internal string TrackerMetadataForMessage(ServerMessage message) => message switch
        {
            ServerMessage.TransactionUpdate(var transactionUpdate) => $"type={nameof(ServerMessage.TransactionUpdate)},reducer={transactionUpdate.ReducerCall.ReducerName}",
            _ => $"type={message.GetType().Name}"
        };

#if UNITY_WEBGL && !UNITY_EDITOR
        internal IEnumerator ParseMessages()
#else
        internal void ParseMessages()
#endif
        {
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
            }

            IEnumerable<(IRemoteTableHandle, TableUpdate)> GetTables(DatabaseUpdate updates)
            {
                foreach (var update in updates.Tables)
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

            ParsedDatabaseUpdate ParseLegacySubscription(InitialSubscription initSub)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                // This is all of the inserts
                int cap = initSub.DatabaseUpdate.Tables.Sum(a => (int)a.NumRows);

                // First apply all of the state
                foreach (var (table, update) in GetTables(initSub.DatabaseUpdate))
                {
                    table.ParseInsertOnly(update, dbOps);
                }
                return dbOps;
            }

            /// <summary>
            /// TODO: the dictionary is here for backwards compatibility and can be removed
            /// once we get rid of legacy subscriptions.
            /// </summary>
            ParsedDatabaseUpdate ParseSubscribeMultiApplied(SubscribeMultiApplied subscribeMultiApplied)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                foreach (var (table, update) in GetTables(subscribeMultiApplied.Update))
                {
                    table.ParseInsertOnly(update, dbOps);
                }
                return dbOps;
            }

            ParsedDatabaseUpdate ParseUnsubscribeMultiApplied(UnsubscribeMultiApplied unsubMultiApplied)
            {
                var dbOps = ParsedDatabaseUpdate.New();

                foreach (var (table, update) in GetTables(unsubMultiApplied.Update))
                {
                    table.ParseDeleteOnly(update, dbOps);
                }

                return dbOps;
            }

            ParsedDatabaseUpdate ParseDatabaseUpdate(DatabaseUpdate updates)
            {
                var dbOps = ParsedDatabaseUpdate.New();

                foreach (var (table, update) in GetTables(updates))
                {
                    table.Parse(update, dbOps);
                }
                return dbOps;
            }

            void ParseOneOffQuery(OneOffQueryResponse resp)
            {
                /// This case does NOT produce a list of DBOps, because it should not modify the client cache state!
                var messageId = new Guid(resp.MessageId.ToArray());

                if (!waitingOneOffQueries.Remove(messageId, out var resultSource))
                {
                    Log.Error($"Response to unknown one-off-query: {messageId}");
                    return;
                }

                resultSource.SetResult(resp);
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
                    case ServerMessage.InitialSubscription(var initSub):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(initSub.RequestId, unparsed.timestamp);
                        dbOps = ParseLegacySubscription(initSub);
                        break;
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        break;
                    case ServerMessage.SubscribeMultiApplied(var subscribeMultiApplied):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(subscribeMultiApplied.RequestId, unparsed.timestamp);
                        dbOps = ParseSubscribeMultiApplied(subscribeMultiApplied);
                        break;
                    case ServerMessage.SubscriptionError(var subscriptionError):
                        // do nothing; main thread will warn.
                        if (subscriptionError.RequestId.HasValue)
                        {
                            stats.SubscriptionRequestTracker.FinishTrackingRequest(subscriptionError.RequestId.Value, unparsed.timestamp);
                        }
                        break;
                    case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                        // do nothing; main thread will warn.
                        break;
                    case ServerMessage.UnsubscribeMultiApplied(var unsubscribeMultiApplied):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(unsubscribeMultiApplied.RequestId, unparsed.timestamp);
                        dbOps = ParseUnsubscribeMultiApplied(unsubscribeMultiApplied);
                        break;
                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        // Convert the generic event arguments in to a domain specific event object
                        var hostDuration = (TimeSpan)transactionUpdate.TotalHostExecutionDuration;
                        stats.AllReducersTracker.InsertRequest(hostDuration, $"reducer={transactionUpdate.ReducerCall.ReducerName}");

                        try
                        {
                            reducerEvent = new(
                                (DateTimeOffset)transactionUpdate.Timestamp,
                                transactionUpdate.Status switch
                                {
                                    UpdateStatus.Committed => Committed,
                                    UpdateStatus.OutOfEnergy => OutOfEnergy,
                                    UpdateStatus.Failed(var reason) => new Status.Failed(reason),
                                    _ => throw new InvalidOperationException()
                                },
                                transactionUpdate.CallerIdentity,
                                transactionUpdate.CallerConnectionId,
                                transactionUpdate.EnergyQuantaUsed.Quanta,
                                ToReducer(transactionUpdate));
                        }
                        catch (Exception)
                        {
                            // Failing to parse the ReducerEvent is fine, it just means we should
                            // call downstream stuff with an UnknownTransaction.
                            // See ApplyMessage
                        }

                        var callerIdentity = transactionUpdate.CallerIdentity;
                        if (callerIdentity == Identity && transactionUpdate.CallerConnectionId == ConnectionId)
                        {
                            // This was a request that we initiated
                            var requestId = transactionUpdate.ReducerCall.RequestId;
                            // Make sure we mark the request as having finished when it came off the wire.
                            // That's why we use unparsed.timestamp, rather than DateTime.UtcNow.
                            // See ReducerRequestTracker's comment.
                            if (!stats.ReducerRequestTracker.FinishTrackingRequest(requestId, unparsed.timestamp))
                            {
                                Log.Warn($"Failed to finish tracking reducer request: {requestId}");
                            }
                        }

                        if (transactionUpdate.Status is UpdateStatus.Committed(var committed))
                        {
                            dbOps = ParseDatabaseUpdate(committed);
                        }

                        break;
                    case ServerMessage.TransactionUpdateLight(var update):
                        dbOps = ParseDatabaseUpdate(update.Update);
                        break;
                    case ServerMessage.IdentityToken(var identityToken):
                        break;
                    case ServerMessage.OneOffQueryResponse(var resp):
                        ParseOneOffQuery(resp);
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

            // Only try to close if the connection is active
            if (webSocket.IsConnected)
            {
                webSocket.Close();
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
                case ServerMessage.InitialSubscription(var initialSubscription):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);

                        if (legacySubscriptions.TryGetValue(initialSubscription.RequestId, out var subscription))
                        {
                            try
                            {
                                subscription.OnApplied(eventContext, new SubscriptionAppliedType.LegacyActive(new()));
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }
                        break;
                    }

                case ServerMessage.SubscribeApplied(var subscribeApplied):
                    Log.Warn($"Unexpected SubscribeApplied (we only expect to get SubscribeMultiApplied): {subscribeApplied}");
                    break;

                case ServerMessage.SubscribeMultiApplied(var subscribeMultiApplied):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(subscribeMultiApplied.QueryId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnApplied(eventContext, new SubscriptionAppliedType.Active(subscribeMultiApplied.QueryId));
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
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
                        if (subscriptionError.QueryId.HasValue)
                        {
                            if (subscriptions.TryGetValue(subscriptionError.QueryId.Value, out var subscription))
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
                        }
                        else
                        {
                            Log.Warn("Received general subscription failure, disconnecting.");
                            Disconnect();
                        }

                        break;
                    }

                case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                    Log.Warn($"Unexpected UnsubscribeApplied (we only expect to get UnsubscribeMultiApplied): {unsubscribeApplied}");
                    break;

                case ServerMessage.UnsubscribeMultiApplied(var unsubscribeMultiApplied):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.UnsubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(unsubscribeMultiApplied.QueryId.Id, out var subscription))
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
                    }
                    break;

                case ServerMessage.TransactionUpdateLight(var update):
                    {
                        var eventContext = ToEventContext(new Event<Reducer>.UnknownTransaction());
                        ApplyUpdate(eventContext, dbOps);

                        break;
                    }

                case ServerMessage.TransactionUpdate(var transactionUpdate):
                    {
                        if (parsed.reducerEvent is { } reducerEvent)
                        {
                            var legacyEventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                            ApplyUpdate(legacyEventContext, dbOps);
                            var eventContext = ToReducerEventContext(reducerEvent);
                            Dispatch(eventContext, reducerEvent.Reducer);
                            // don't invoke OnUnhandledReducerError, that's [Obsolete].
                        }
                        else
                        {
                            var legacyEventContext = ToEventContext(new Event<Reducer>.UnknownTransaction());
                            ApplyUpdate(legacyEventContext, dbOps);
                        }
                        break;
                    }
                case ServerMessage.IdentityToken(var identityToken):
                    try
                    {
                        Identity = identityToken.Identity;
                        onConnect?.Invoke(identityToken.Identity, identityToken.Token);
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                    break;

                case ServerMessage.OneOffQueryResponse:
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

        // TODO: this should become [Obsolete] but for now is used by autogenerated code.
        void IDbConnection.InternalCallReducer<T>(T args, CallReducerFlags flags)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call reducer, not connected to server!");
                return;
            }

            webSocket.Send(new ClientMessage.CallReducer(new CallReducer(
                args.ReducerName,
                IStructuralReadWrite.ToBytes(args).ToList(),
                stats.ReducerRequestTracker.StartTrackingRequest(args.ReducerName),
                (byte)flags
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
                args.ProcedureName,
                IStructuralReadWrite.ToBytes(args).ToList(),
                requestId,
                0 // flags - assuming default for now
            )));
        }

        void IDbConnection.LegacySubscribe(ISubscriptionHandle handle, string[] querySqls)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return;
            }

            var id = stats.SubscriptionRequestTracker.StartTrackingRequest();
            legacySubscriptions[id] = handle;
            webSocket.Send(new ClientMessage.Subscribe(
                new Subscribe
                {
                    RequestId = id,
                    QueryStrings = querySqls.ToList()
                }
            ));
        }

        QueryId? IDbConnection.Subscribe(ISubscriptionHandle handle, string[] querySqls)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return null;
            }

            var id = stats.SubscriptionRequestTracker.StartTrackingRequest();
            // We use a distinct ID from the request ID as a sanity check that we're not
            // casting request IDs to query IDs anywhere in the new code path.
            var queryId = queryIdAllocator.Next();
            subscriptions[queryId] = handle;
            webSocket.Send(new ClientMessage.SubscribeMulti(
                new SubscribeMulti
                {
                    RequestId = id,
                    QueryStrings = querySqls.ToList(),
                    QueryId = new QueryId(queryId),
                }
            ));
            return new QueryId(queryId);
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("SELECT * FROM table WHERE sender = \"bob\"");
        [Obsolete("This is replaced by ctx.Db.TableName.RemoteQuery(\"WHERE ...\")", false)]
        public Task<T[]> OneOffQuery<T>(string query) where T : IStructuralReadWrite, new() =>
            ((IDbConnection)this).RemoteQuery<T>(query);

        async Task<T[]> IDbConnection.RemoteQuery<T>(string query)
        {
            var messageId = Guid.NewGuid();
            var resultSource = new TaskCompletionSource<OneOffQueryResponse>();
            waitingOneOffQueries[messageId] = resultSource;

            // unsanitized here, but writes will be prevented serverside.
            // the best they can do is send multiple selects, which will just result in them getting no data back.

            var requestId = stats.OneOffRequestTracker.StartTrackingRequest();
            webSocket.Send(new ClientMessage.OneOffQuery(new OneOffQuery
            {
                MessageId = messageId.ToByteArray().ToList(),
                QueryString = query,
            }));

            // Suspend for an arbitrary amount of time
            var result = await resultSource.Task;

            if (!stats.OneOffRequestTracker.FinishTrackingRequest(requestId))
            {
                Log.Warn($"Failed to finish tracking one off request: {requestId}");
            }

            T[] LogAndThrow(string error)
            {
                error = $"While processing one-off-query `{query}`, ID {messageId}: {error}";
                Log.Error(error);
                throw new Exception(error);
            }

            // The server got back to us
            if (result.Error != null && result.Error != "")
            {
                return LogAndThrow($"Server error: {result.Error}");
            }

            if (result.Tables.Count != 1)
            {
                return LogAndThrow($"Expected a single table, but got {result.Tables.Count}");
            }

            var resultTable = result.Tables[0];
            var cacheTable = Db.GetTable(resultTable.TableName);

            if (cacheTable?.ClientTableType != typeof(T))
            {
                return LogAndThrow($"Mismatched result type, expected {typeof(T)} but got {resultTable.TableName}");
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

        void IDbConnection.Unsubscribe(QueryId queryId)
        {
            if (!subscriptions.ContainsKey(queryId.Id))
            {
                Log.Warn($"Unsubscribing from a subscription that the DbConnection does not know about, with QueryId {queryId.Id}");
            }

            var requestId = stats.SubscriptionRequestTracker.StartTrackingRequest();

            webSocket.Send(new ClientMessage.UnsubscribeMulti(new()
            {
                RequestId = requestId,
                QueryId = queryId
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
