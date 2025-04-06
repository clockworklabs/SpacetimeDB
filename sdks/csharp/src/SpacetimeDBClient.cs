using System;
using System.Collections;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;
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
        internal void Connect(string? token, string uri, string addressOrName, Compression compression, bool light);

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

        private readonly Dictionary<Guid, TaskCompletionSource<OneOffQueryResponse>> waitingOneOffQueries = new();

        private bool isClosing;
        private readonly Thread networkMessageProcessThread;
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
                SpacetimeDBNetworkManager._instance.StartCoroutine(PreProcessMessages());
#endif
#endif
#if !(UNITY_WEBGL && !UNITY_EDITOR)
            networkMessageProcessThread = new Thread(PreProcessMessages);
            networkMessageProcessThread.Start();
#endif
        }

        struct UnprocessedMessage
        {
            public byte[] bytes;
            public DateTime timestamp;
        }

        struct ProcessedDatabaseUpdate
        {
            // Map: table handles -> (primary key -> DbValue).
            // If a particular table has no primary key, the "primary key" is just a byte[]
            // storing the BSATN encoding of the row.
            // See Decode(...).
            public Dictionary<IRemoteTableHandle, MultiDictionaryDelta<object, DbValue>> Updates;

            // Can't override the default constructor. Make sure you use this one!
            public static ProcessedDatabaseUpdate New()
            {
                ProcessedDatabaseUpdate result;
                result.Updates = new();
                return result;
            }

            public MultiDictionaryDelta<object, DbValue> DeltaForTable(IRemoteTableHandle table)
            {
                if (!Updates.TryGetValue(table, out var delta))
                {
                    // Make sure we use GenericEqualityComparer here, since it handles byte[]s and arbitrary primary key types
                    // correctly.
                    delta = new MultiDictionaryDelta<object, DbValue>(GenericEqualityComparer.Instance, DbValueComparer.Instance);
                    Updates[table] = delta;
                }

                return delta;
            }
        }

        struct ProcessedMessage
        {
            public ServerMessage message;
            public ProcessedDatabaseUpdate dbOps;
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

        /// <summary>
        /// Decode a row for a table, producing a primary key.
        /// If the table has a specific column marked `#[primary_key]`, use that.
        /// If not, the BSATN for the entire row is used instead.
        /// </summary>
        /// <param name="table"></param>
        /// <param name="bin"></param>
        /// <param name="primaryKey"></param>
        /// <returns></returns>
        static DbValue Decode(IRemoteTableHandle table, byte[] bin, out object primaryKey)
        {
            var obj = table.DecodeValue(bin);
            // TODO(1.1): we should exhaustively check that GenericEqualityComparer works
            // for all types that are allowed to be primary keys.
            var primaryKey_ = table.GetPrimaryKey(obj);
            primaryKey_ ??= bin;
            primaryKey = primaryKey_;
            return new(obj, bin);
        }

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

        private static QueryUpdate DecompressDecodeQueryUpdate(CompressableQueryUpdate update)
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

        private static IEnumerable<byte[]> BsatnRowListIter(BsatnRowList list)
        {
            var rowsData = list.RowsData;

            return list.SizeHint switch
            {
                RowSizeHint.FixedSize(var size) => Enumerable
                    .Range(0, rowsData.Count / size)
                    .Select(index => rowsData.Skip(index * size).Take(size).ToArray()),

                RowSizeHint.RowOffsets(var offsets) => offsets.Zip(
                    offsets.Skip(1).Append((ulong)rowsData.Count),
                    (start, end) => rowsData.Take((int)end).Skip((int)start).ToArray()
                ),

                _ => throw new InvalidOperationException("Unknown RowSizeHint variant"),
            };
        }

#if UNITY_WEBGL && !UNITY_EDITOR
        IEnumerator PreProcessMessages()
#else
        void PreProcessMessages()
#endif
        {
            while (!isClosing)
            {
                
#if UNITY_WEBGL && !UNITY_EDITOR
                if (_messageQueue.Count > 0)
#else
                try
#endif
                {
                    var message = _messageQueue.Take(_preProcessCancellationToken);
                    var preprocessedMessage = PreProcessMessage(message);
                    _preProcessedNetworkMessages.Add(preprocessedMessage, _preProcessCancellationToken);
                }
#if UNITY_WEBGL && !UNITY_EDITOR
                else
                {
                    yield return new UnityEngine.WaitForSeconds(0.1f);
                }
#else
                catch (OperationCanceledException)
                {
                    return; // Normal shutdown
                }
#endif
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

            ProcessedDatabaseUpdate PreProcessLegacySubscription(InitialSubscription initSub)
            {
                var dbOps = ProcessedDatabaseUpdate.New();
                // This is all of the inserts
                int cap = initSub.DatabaseUpdate.Tables.Sum(a => (int)a.NumRows);

                // First apply all of the state
                foreach (var (table, update) in GetTables(initSub.DatabaseUpdate))
                {
                    PreProcessInsertOnlyTable(table, update, dbOps);
                }
                return dbOps;
            }

            /// <summary>
            /// TODO: the dictionary is here for backwards compatibility and can be removed
            /// once we get rid of legacy subscriptions.
            /// </summary>
            ProcessedDatabaseUpdate PreProcessSubscribeMultiApplied(SubscribeMultiApplied subscribeMultiApplied)
            {
                var dbOps = ProcessedDatabaseUpdate.New();
                foreach (var (table, update) in GetTables(subscribeMultiApplied.Update))
                {
                    PreProcessInsertOnlyTable(table, update, dbOps);
                }
                return dbOps;
            }

            void PreProcessInsertOnlyTable(IRemoteTableHandle table, TableUpdate update, ProcessedDatabaseUpdate dbOps)
            {
                var delta = dbOps.DeltaForTable(table);

                foreach (var cqu in update.Updates)
                {
                    var qu = DecompressDecodeQueryUpdate(cqu);
                    if (qu.Deletes.RowsData.Count > 0)
                    {
                        Log.Warn("Non-insert during an insert-only server message!");
                    }
                    foreach (var bin in BsatnRowListIter(qu.Inserts))
                    {
                        var obj = Decode(table, bin, out var pk);
                        delta.Add(pk, obj);
                    }
                }
            }

            void PreProcessDeleteOnlyTable(IRemoteTableHandle table, TableUpdate update, ProcessedDatabaseUpdate dbOps)
            {
                var delta = dbOps.DeltaForTable(table);
                foreach (var cqu in update.Updates)
                {
                    var qu = DecompressDecodeQueryUpdate(cqu);
                    if (qu.Inserts.RowsData.Count > 0)
                    {
                        Log.Warn("Non-delete during a delete-only operation!");
                    }
                    foreach (var bin in BsatnRowListIter(qu.Deletes))
                    {
                        var obj = Decode(table, bin, out var pk);
                        delta.Remove(pk, obj);
                    }
                }
            }

            void PreProcessTable(IRemoteTableHandle table, TableUpdate update, ProcessedDatabaseUpdate dbOps)
            {
                var delta = dbOps.DeltaForTable(table);
                foreach (var cqu in update.Updates)
                {
                    var qu = DecompressDecodeQueryUpdate(cqu);

                    // Because we are accumulating into a MultiDictionaryDelta that will be applied all-at-once
                    // to the table, it doesn't matter that we call Add before Remove here.

                    foreach (var bin in BsatnRowListIter(qu.Inserts))
                    {
                        var obj = Decode(table, bin, out var pk);
                        delta.Add(pk, obj);
                    }
                    foreach (var bin in BsatnRowListIter(qu.Deletes))
                    {
                        var obj = Decode(table, bin, out var pk);
                        delta.Remove(pk, obj);
                    }
                }

            }

            ProcessedDatabaseUpdate PreProcessUnsubscribeMultiApplied(UnsubscribeMultiApplied unsubMultiApplied)
            {
                var dbOps = ProcessedDatabaseUpdate.New();

                foreach (var (table, update) in GetTables(unsubMultiApplied.Update))
                {
                    PreProcessDeleteOnlyTable(table, update, dbOps);
                }

                return dbOps;
            }

            ProcessedDatabaseUpdate PreProcessDatabaseUpdate(DatabaseUpdate updates)
            {
                var dbOps = ProcessedDatabaseUpdate.New();

                foreach (var (table, update) in GetTables(updates))
                {
                    PreProcessTable(table, update, dbOps);
                }
                return dbOps;
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

            ProcessedMessage PreProcessMessage(UnprocessedMessage unprocessed)
            {
                var dbOps = ProcessedDatabaseUpdate.New();

                var message = DecompressDecodeMessage(unprocessed.bytes);

                ReducerEvent<Reducer>? reducerEvent = default;

                switch (message)
                {
                    case ServerMessage.InitialSubscription(var initSub):
                        dbOps = PreProcessLegacySubscription(initSub);
                        break;
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        break;
                    case ServerMessage.SubscribeMultiApplied(var subscribeMultiApplied):
                        dbOps = PreProcessSubscribeMultiApplied(subscribeMultiApplied);
                        break;
                    case ServerMessage.SubscriptionError(var subscriptionError):
                        // do nothing; main thread will warn.
                        break;
                    case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                        // do nothing; main thread will warn.
                        break;
                    case ServerMessage.UnsubscribeMultiApplied(var unsubscribeMultiApplied):
                        dbOps = PreProcessUnsubscribeMultiApplied(unsubscribeMultiApplied);
                        break;
                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        // Convert the generic event arguments in to a domain specific event object
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
                            // See OnProcessMessageComplete.
                        }

                        if (transactionUpdate.Status is UpdateStatus.Committed(var committed))
                        {
                            dbOps = PreProcessDatabaseUpdate(committed);
                        }
                        break;
                    case ServerMessage.TransactionUpdateLight(var update):
                        dbOps = PreProcessDatabaseUpdate(update.Update);
                        break;
                    case ServerMessage.IdentityToken(var identityToken):
                        break;
                    case ServerMessage.OneOffQueryResponse(var resp):
                        PreProcessOneOffQuery(resp);
                        break;

                    default:
                        throw new InvalidOperationException();
                }

                return new ProcessedMessage { message = message, dbOps = dbOps, timestamp = unprocessed.timestamp, reducerEvent = reducerEvent };
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
        void IDbConnection.Connect(string? token, string uri, string addressOrName, Compression compression, bool light)
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
                        await webSocket.Connect(token, uri, addressOrName, ConnectionId, compression, light);
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


        private void OnMessageProcessCompleteUpdate(IEventContext eventContext, ProcessedDatabaseUpdate dbOps)
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

        private void OnMessageProcessComplete(ProcessedMessage processed)
        {
            var message = processed.message;
            var dbOps = processed.dbOps;
            var timestamp = processed.timestamp;

            switch (message)
            {
                case ServerMessage.InitialSubscription(var initialSubscription):
                    {
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.InitialSubscription)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(initialSubscription.RequestId);
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);

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
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.SubscribeApplied)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(subscribeMultiApplied.RequestId);
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
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
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.SubscriptionError)}");
                        if (subscriptionError.RequestId.HasValue)
                        {
                            stats.SubscriptionRequestTracker.FinishTrackingRequest(subscriptionError.RequestId.Value);
                        }
                        // TODO: should I use a more specific exception type here?
                        var exception = new Exception(subscriptionError.Error);
                        var eventContext = ToErrorContext(exception);
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeError(exception));
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
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
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.UnsubscribeApplied)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(unsubscribeMultiApplied.RequestId);
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.UnsubscribeApplied());
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
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
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.TransactionUpdateLight)}");

                        var eventContext = ToEventContext(new Event<Reducer>.UnknownTransaction());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);

                        break;
                    }

                case ServerMessage.TransactionUpdate(var transactionUpdate):
                    {
                        var reducer = transactionUpdate.ReducerCall.ReducerName;
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.TransactionUpdate)},reducer={reducer}");
                        var hostDuration = (TimeSpan)transactionUpdate.TotalHostExecutionDuration;
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

                        if (processed.reducerEvent is { } reducerEvent)
                        {
                            var legacyEventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                            OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
                            var eventContext = ToReducerEventContext(reducerEvent);
                            Dispatch(eventContext, reducerEvent.Reducer);
                            // don't invoke OnUnhandledReducerError, that's [Obsolete].
                        }
                        else
                        {
                            var legacyEventContext = ToEventContext(new Event<Reducer>.UnknownTransaction());
                            OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
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

            return BsatnRowListIter(resultTable.Rows)
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
    internal readonly struct DbValue
    {
        public readonly IStructuralReadWrite value;
        public readonly byte[] bytes;

        public DbValue(IStructuralReadWrite value, byte[] bytes)
        {
            this.value = value;
            this.bytes = bytes;
        }

        // TODO: having a nice ToString here would give better way better errors when applying table deltas,
        // but it's tricky to do that generically.
    }

    /// <summary>
    /// DbValue comparer that uses BSATN-encoded records to compare DbValues for equality.
    /// </summary>
    internal readonly struct DbValueComparer : IEqualityComparer<DbValue>
    {
        public static DbValueComparer Instance = new();

        public bool Equals(DbValue x, DbValue y) =>
            ByteArrayComparer.Instance.Equals(x.bytes, y.bytes);

        public int GetHashCode(DbValue obj) =>
            ByteArrayComparer.Instance.GetHashCode(obj.bytes);
    }
}
