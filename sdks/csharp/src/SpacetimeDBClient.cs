using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Thread = System.Threading.Thread;

namespace SpacetimeDB
{
    public sealed class DbConnectionBuilder<DbConnection, Reducer>
        where DbConnection : DbConnectionBase<DbConnection, Reducer>, new()
    {
        readonly DbConnection conn = new();

        string? uri;
        string? nameOrAddress;
        string? token;
        Compression? compression;
        bool light;

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
            conn.Connect(token, uri, nameOrAddress, compression ?? Compression.Brotli, light);
#if UNITY_5_3_OR_NEWER
            if (SpacetimeDBNetworkManager._instance != null)
            {
                SpacetimeDBNetworkManager._instance.AddConnection(conn);
            }
#endif
            return conn;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithUri(string uri)
        {
            this.uri = uri;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithModuleName(string nameOrAddress)
        {
            this.nameOrAddress = nameOrAddress;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithToken(string? token)
        {
            this.token = token;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithCompression(Compression compression)
        {
            this.compression = compression;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithLightMode(bool light)
        {
            this.light = light;
            return this;
        }

        public delegate void ConnectCallback(DbConnection conn, Identity identity, string token);

        public DbConnectionBuilder<DbConnection, Reducer> OnConnect(ConnectCallback cb)
        {
            conn.onConnect += (identity, token) => cb.Invoke(conn, identity, token);
            return this;
        }

        public delegate void ConnectErrorCallback(Exception e);

        public DbConnectionBuilder<DbConnection, Reducer> OnConnectError(ConnectErrorCallback cb)
        {
            conn.webSocket.OnConnectError += (e) => cb.Invoke(e);
            return this;
        }

        public delegate void DisconnectCallback(DbConnection conn, Exception? e);

        public DbConnectionBuilder<DbConnection, Reducer> OnDisconnect(DisconnectCallback cb)
        {
            conn.webSocket.OnClose += (e) => cb.Invoke(conn, e);
            return this;
        }
    }

    public interface IDbConnection
    {
        internal void LegacySubscribe(ISubscriptionHandle handle, string[] querySqls);
        internal void Subscribe(ISubscriptionHandle handle, string querySql);
        internal void Unsubscribe(QueryId queryId);
        void FrameTick();
        void Disconnect();

        internal Task<T[]> RemoteQuery<T>(string query) where T : IDatabaseRow, new();
    }

    public abstract class DbConnectionBase<DbConnection, Reducer> : IDbConnection
        where DbConnection : DbConnectionBase<DbConnection, Reducer>, new()
    {
        public static DbConnectionBuilder<DbConnection, Reducer> Builder() => new();

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

        /// <summary>
        /// Invoked when a reducer is returned with an error and has no client-side handler.
        /// </summary>
        [Obsolete]
        public event Action<ReducerEvent<Reducer>>? onUnhandledReducerError;

        public readonly Address Address = Address.Random();
        public Identity? Identity { get; private set; }

        internal WebSocket webSocket;
        private bool connectionClosed;
        protected readonly ClientCache clientDB;

        protected abstract Reducer ToReducer(TransactionUpdate update);
        protected abstract IEventContext ToEventContext(Event<Reducer> reducerEvent);

        private readonly Dictionary<Guid, TaskCompletionSource<OneOffQueryResponse>> waitingOneOffQueries = new();

        private bool isClosing;
        private readonly Thread networkMessageProcessThread;
        public readonly Stats stats = new();

        protected DbConnectionBase()
        {
            clientDB = new(this);

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
#endif

            networkMessageProcessThread = new Thread(PreProcessMessages);
            networkMessageProcessThread.Start();
        }

        struct UnprocessedMessage
        {
            public byte[] bytes;
            public DateTime timestamp;
        }

        struct ProcessedMessage
        {
            public ServerMessage message;
            public Dictionary<IRemoteTableHandle, IDbOps>? dbOps;
            public DateTime timestamp;
            public ReducerEvent<Reducer>? reducerEvent;
        }

        private readonly BlockingCollection<UnprocessedMessage> _messageQueue =
            new(new ConcurrentQueue<UnprocessedMessage>());

        private readonly BlockingCollection<ProcessedMessage> _preProcessedNetworkMessages =
            new(new ConcurrentQueue<ProcessedMessage>());

        internal static bool IsTesting;
        internal bool HasPreProcessedMessage => _preProcessedNetworkMessages.Count > 0;

        private readonly CancellationTokenSource _preProcessCancellationTokenSource = new();
        private CancellationToken _preProcessCancellationToken => _preProcessCancellationTokenSource.Token;

        private static readonly Status Committed = new Status.Committed(default);
        private static readonly Status OutOfEnergy = new Status.OutOfEnergy(default);

        enum CompressionAlgos : byte
        {
            None = 0,
            Brotli = 1,
            Gzip = 2,
        }

        private static BrotliStream BrotliReader(Stream stream)
        {
            return new BrotliStream(stream, CompressionMode.Decompress);
        }

        private static GZipStream GzipReader(Stream stream)
        {
            return new GZipStream(stream, CompressionMode.Decompress);
        }

        private static ServerMessage DecompressDecodeMessage(byte[] bytes)
        {
            using var stream = new MemoryStream(bytes);

            // The stream will never be empty. It will at least contain the compression algo.
            var compression = (CompressionAlgos)stream.ReadByte();
            // Conditionally decompress and decode.
            Stream decompressedStream = compression switch
            {
                CompressionAlgos.None => stream,
                CompressionAlgos.Brotli => BrotliReader(stream),
                CompressionAlgos.Gzip => GzipReader(stream),
                _ => throw new InvalidOperationException("Unknown compression type"),
            };

            return new ServerMessage.BSATN().Read(new BinaryReader(decompressedStream));
        }

        void PreProcessMessages()
        {
            while (!isClosing)
            {
                try
                {
                    var message = _messageQueue.Take(_preProcessCancellationToken);
                    var preprocessedMessage = PreProcessMessage(message);
                    _preProcessedNetworkMessages.Add(preprocessedMessage, _preProcessCancellationToken);
                }
                catch (OperationCanceledException)
                {
                    return; // Normal shutdown
                }
            }

            Dictionary<IRemoteTableHandle, IDbOps> UpdatesToDbOps(IEnumerable<TableUpdate> updates, Func<IRemoteTableHandle, IEnumerable<QueryUpdate>, IDbOps> handleTableUpdates)
            {
                IEnumerable<(IRemoteTableHandle table, IEnumerable<QueryUpdate> updates)> GetGroupedUpdates(IGrouping<string, TableUpdate> tableUpdates)
                {
                    var tableName = tableUpdates.Key;
                    var table = clientDB.GetTable(tableName);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableName}");
                    }
                    else
                    {
                        yield return (table, tableUpdates.SelectMany(update => update.Updates).Select(DecompressDecodeQueryUpdate));
                    }
                }

                return updates.GroupBy(update => update.TableName).SelectMany(GetGroupedUpdates).ToDictionary(
                    tableUpdates => tableUpdates.table,
                    tableUpdates => handleTableUpdates(tableUpdates.table, tableUpdates.updates)
                );
            }

            void PreProcessOneOffQuery(OneOffQueryResponse resp)
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

            static QueryUpdate DecompressDecodeQueryUpdate(CompressableQueryUpdate update)
            {
                Stream decompressedStream;

                switch (update)
                {
                    case CompressableQueryUpdate.Uncompressed(var qu):
                        return qu;

                    case CompressableQueryUpdate.Brotli(var bytes):
                        decompressedStream = BrotliReader(new MemoryStream(bytes.ToArray()));
                        break;

                    case CompressableQueryUpdate.Gzip(var bytes):
                        decompressedStream = GzipReader(new MemoryStream(bytes.ToArray()));
                        break;

                    default:
                        throw new InvalidOperationException();
                }

                return new QueryUpdate.BSATN().Read(new BinaryReader(decompressedStream));
            }

            ProcessedMessage PreProcessMessage(UnprocessedMessage unprocessed)
            {
                Dictionary<IRemoteTableHandle, IDbOps>? dbOps = null;

                var message = DecompressDecodeMessage(unprocessed.bytes);

                ReducerEvent<Reducer>? reducerEvent = null;

                switch (message)
                {
                    case ServerMessage.InitialSubscription(var initSub):
                        dbOps = UpdatesToDbOps(initSub.DatabaseUpdate.Tables, (table, updates) => table.PreProcessInsertOnlyTable(updates));
                        break;
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        dbOps = UpdatesToDbOps(new[] { subscribeApplied.Rows.TableRows }, (table, updates) => table.PreProcessInsertOnlyTable(updates));
                        break;
                    case ServerMessage.SubscriptionError(var subscriptionError):
                        break;
                    case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                        dbOps = UpdatesToDbOps(new[] { unsubscribeApplied.Rows.TableRows }, (table, updates) => table.PreProcessUnsubscribeApplied(updates));
                        break;
                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        // Convert the generic event arguments in to a domain specific event object
                        try
                        {
                            reducerEvent = new(
                                DateTimeOffset.FromUnixTimeMilliseconds(
                                    (long)transactionUpdate.Timestamp.Microseconds / 1000
                                ),
                                transactionUpdate.Status switch
                                {
                                    UpdateStatus.Committed => Committed,
                                    UpdateStatus.OutOfEnergy => OutOfEnergy,
                                    UpdateStatus.Failed(var reason) => new Status.Failed(reason),
                                    _ => throw new InvalidOperationException(),
                                },
                                transactionUpdate.CallerIdentity,
                                transactionUpdate.CallerAddress,
                                transactionUpdate.EnergyQuantaUsed.Quanta,
                                ToReducer(transactionUpdate)
                            );
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
                        }

                        if (transactionUpdate.Status is UpdateStatus.Committed(var committed))
                        {
                            dbOps = UpdatesToDbOps(committed.Tables, (table, updates) => table.PreProcessTableUpdate(updates));
                        }
                        break;
                    case ServerMessage.TransactionUpdateLight(var update):
                        dbOps = UpdatesToDbOps(update.Update.Tables, (table, updates) => table.PreProcessTableUpdate(updates));
                        break;
                    case ServerMessage.IdentityToken(var identityToken):
                        break;
                    case ServerMessage.OneOffQueryResponse(var resp):
                        PreProcessOneOffQuery(resp);
                        break;

                    default:
                        throw new InvalidOperationException();
                }

                return new ProcessedMessage
                {
                    message = message,
                    dbOps = dbOps,
                    timestamp = unprocessed.timestamp,
                    reducerEvent = reducerEvent,
                };
            }
        }

        public void Disconnect()
        {
            isClosing = true;
            connectionClosed = true;
            webSocket.Close();
            _preProcessCancellationTokenSource.Cancel();
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="uri"> URI of the SpacetimeDB server (ex: https://testnet.spacetimedb.com)
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        internal void Connect(string? token, string uri, string addressOrName, Compression compression, bool light)
        {
            isClosing = false;

            uri = uri.Replace("http://", "ws://");
            uri = uri.Replace("https://", "wss://");
            if (!uri.StartsWith("ws://") && !uri.StartsWith("wss://"))
            {
                uri = $"ws://{uri}";
            }

            Log.Info($"SpacetimeDBClient: Connecting to {uri} {addressOrName}");
            if (!IsTesting)
            {
                Task.Run(async () =>
                {
                    try
                    {
                        await webSocket.Connect(token, uri, addressOrName, Address, compression, light);
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
                });
            }
        }

        private void OnMessageProcessCompleteUpdate(IEventContext eventContext, IEnumerable<IDbOps> dbOps)
        {
            foreach (var tableOps in dbOps)
            {
                tableOps.OnMessageProcessCompleteUpdate(eventContext);
            }
        }

        protected abstract bool Dispatch(IEventContext context, Reducer reducer);

        private void OnMessageProcessComplete(ProcessedMessage processed)
        {
            if (processed.dbOps?.Values is not { } dbOps)
            {
                return;
            }

            foreach (var tableOps in dbOps)
            {
                tableOps.CalculateStateDiff();
            }

            var message = processed.message;
            var timestamp = processed.timestamp;

            switch (message)
            {
                case ServerMessage.InitialSubscription(var initialSubscription):
                    {
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.InitialSubscription)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(initialSubscription.RequestId);
                        var eventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
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
                    {
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.SubscribeApplied)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(subscribeApplied.RequestId);
                        var eventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
                        if (subscriptions.TryGetValue(subscribeApplied.QueryId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnApplied(eventContext, new SubscriptionAppliedType.Active(subscribeApplied.QueryId));
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
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.SubscriptionError)}");
                        if (subscriptionError.RequestId.HasValue)
                        {
                            stats.SubscriptionRequestTracker.FinishTrackingRequest(subscriptionError.RequestId.Value);
                        }
                        // TODO: should I use a more specific exception type here?
                        var eventContext = ToEventContext(new Event<Reducer>.SubscribeError(new Exception(subscriptionError.Error)));
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
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
                    {
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.UnsubscribeApplied)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(unsubscribeApplied.RequestId);
                        var eventContext = ToEventContext(new Event<Reducer>.UnsubscribeApplied());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
                        if (subscriptions.TryGetValue(unsubscribeApplied.QueryId.Id, out var subscription))
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
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.TransactionUpdateLight)}");

                        var eventContext = ToEventContext(new Event<Reducer>.UnknownTransaction());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);

                        break;
                    }

                case ServerMessage.TransactionUpdate(var transactionUpdate):
                    {
                        var reducer = transactionUpdate.ReducerCall.ReducerName;
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.TransactionUpdate)},reducer={reducer}");
                        var hostDuration = TimeSpan.FromMilliseconds(transactionUpdate.HostExecutionDurationMicros / 1000.0d);
                        stats.AllReducersTracker.InsertRequest(hostDuration, $"reducer={reducer}");
                        var callerIdentity = transactionUpdate.CallerIdentity;
                        if (callerIdentity == Identity)
                        {
                            // This was a request that we initiated
                            var requestId = transactionUpdate.ReducerCall.RequestId;
                            if (!stats.ReducerRequestTracker.FinishTrackingRequest(requestId))
                            {
                                Log.Warn($"Failed to finish tracking reducer request: {requestId}");
                            }
                        }

                        if (processed.reducerEvent is not { } reducerEvent)
                        {
                            // If we are here, an error about unknown reducer should have already been logged, so nothing to do.
                            break;
                        }

                        var eventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);

                        var reducerFound = false;
                        try
                        {
                            reducerFound = Dispatch(eventContext, reducerEvent.Reducer);
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
                        }

                        if (!reducerFound && transactionUpdate.Status is UpdateStatus.Failed(var failed))
                        {
                            try
                            {
                                onUnhandledReducerError?.Invoke(reducerEvent);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
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

                default:
                    throw new InvalidOperationException();
            }
        }

        // Note: this method is called from unit tests.
        internal void OnMessageReceived(byte[] bytes, DateTime timestamp) =>
            _messageQueue.Add(new UnprocessedMessage { bytes = bytes, timestamp = timestamp });

        // TODO: this should become [Obsolete] but for now is used by autogenerated code.
        public void InternalCallReducer<T>(T args, CallReducerFlags flags)
            where T : IReducerArgs, new()
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

        void IDbConnection.Subscribe(ISubscriptionHandle handle, string querySql)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return;
            }

            var id = stats.SubscriptionRequestTracker.StartTrackingRequest();
            // We use a distinct ID from the request ID as a sanity check that we're not
            // casting request IDs to query IDs anywhere in the new code path.
            var queryId = queryIdAllocator.Next();
            subscriptions[queryId] = handle;
            webSocket.Send(new ClientMessage.SubscribeSingle(
                new SubscribeSingle
                {
                    RequestId = id,
                    Query = querySql,
                    QueryId = new QueryId(queryId),
                }
            ));
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("SELECT * FROM table WHERE sender = \"bob\"");
        [Obsolete("This is replaced by ctx.Db.TableName.RemoteQuery(\"WHERE ...\")", false)]
        public Task<T[]> OneOffQuery<T>(string query) where T : IDatabaseRow, new() =>
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
            var cacheTable = clientDB.GetTable(resultTable.TableName);

            if (cacheTable?.ClientTableType != typeof(T))
            {
                return LogAndThrow($"Mismatched result type, expected {typeof(T)} but got {resultTable.TableName}");
            }

            return resultTable.Rows
                .Select(BSATNHelpers.Decode<T>)
                .ToArray();
        }

        public bool IsActive => webSocket.IsConnected;

        public void FrameTick()
        {
            webSocket.Update();
            while (_preProcessedNetworkMessages.TryTake(out var preProcessedMessage))
            {
                OnMessageProcessComplete(preProcessedMessage);
            }
        }

        void IDbConnection.Unsubscribe(QueryId queryId)
        {
            if (!subscriptions.ContainsKey(queryId.Id))
            {
                Log.Warn($"Unsubscribing from a subscription that the DbConnection does not know about, with QueryId {queryId.Id}");
            }

            var requestId = stats.SubscriptionRequestTracker.StartTrackingRequest();

            webSocket.Send(new ClientMessage.Unsubscribe(new()
            {
                RequestId = requestId,
                QueryId = queryId
            }));

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
