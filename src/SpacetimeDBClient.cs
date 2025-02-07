using System;
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
using System.Diagnostics;

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
        internal void Subscribe(ISubscriptionHandle handle, string querySql);
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

        readonly struct DbValue
        {
            public readonly IStructuralReadWrite value;
            public readonly byte[] bytes;

            public DbValue(IStructuralReadWrite value, byte[] bytes)
            {
                this.value = value;
                this.bytes = bytes;
            }
        }

        struct DbOp
        {
            public IRemoteTableHandle table;
            public DbValue? delete;
            public DbValue? insert;
        }

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
            public List<DbOp> dbOps;
            public DateTime timestamp;
            public ReducerEvent<Reducer>? reducerEvent;
        }

        struct PreProcessedMessage
        {
            public ProcessedMessage processed;
            public Dictionary<IRemoteTableHandle, HashSet<byte[]>>? subscriptionInserts;
        }

        private readonly BlockingCollection<UnprocessedMessage> _messageQueue =
            new(new ConcurrentQueue<UnprocessedMessage>());

        private readonly BlockingCollection<PreProcessedMessage> _preProcessedNetworkMessages =
            new(new ConcurrentQueue<PreProcessedMessage>());

        internal static bool IsTesting;
        internal bool HasPreProcessedMessage => _preProcessedNetworkMessages.Count > 0;

        private readonly CancellationTokenSource _preProcessCancellationTokenSource = new();
        private CancellationToken _preProcessCancellationToken => _preProcessCancellationTokenSource.Token;

        static DbValue Decode(IRemoteTableHandle table, byte[] bin, out object? primaryKey)
        {
            var obj = table.DecodeValue(bin);
            primaryKey = table.GetPrimaryKey(obj);
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

            (List<DbOp>, Dictionary<IRemoteTableHandle, HashSet<byte[]>>) PreProcessLegacySubscription(InitialSubscription initSub)
            {
                var dbOps = new List<DbOp>();
                // This is all of the inserts
                int cap = initSub.DatabaseUpdate.Tables.Sum(a => (int)a.NumRows);
                // FIXME: shouldn't this be `new(initSub.DatabaseUpdate.Tables.Length)` ?
                Dictionary<IRemoteTableHandle, HashSet<byte[]>> subscriptionInserts = new(capacity: cap);

                HashSet<byte[]> GetInsertHashSet(IRemoteTableHandle table, int tableSize)
                {
                    if (!subscriptionInserts.TryGetValue(table, out var hashSet))
                    {
                        hashSet = new HashSet<byte[]>(capacity: tableSize, comparer: ByteArrayComparer.Instance);
                        subscriptionInserts[table] = hashSet;
                    }
                    return hashSet;
                }

                // First apply all of the state
                foreach (var (table, update) in GetTables(initSub.DatabaseUpdate))
                {
                    var hashSet = GetInsertHashSet(table, (int)update.NumRows);

                    PreProcessInsertOnlyTable(table, update, dbOps, hashSet);
                }
                return (dbOps, subscriptionInserts);
            }

            /// <summary>
            /// TODO: the dictionary is here for backwards compatibility and can be removed
            /// once we get rid of legacy subscriptions.
            /// </summary>
            (List<DbOp>, Dictionary<IRemoteTableHandle, HashSet<byte[]>>) PreProcessSubscribeApplied(SubscribeApplied subscribeApplied)
            {
                var table = Db.GetTable(subscribeApplied.Rows.TableName) ?? throw new Exception($"Unknown table name: {subscribeApplied.Rows.TableName}");
                var dbOps = new List<DbOp>();
                HashSet<byte[]> inserts = new(comparer: ByteArrayComparer.Instance);

                PreProcessInsertOnlyTable(table, subscribeApplied.Rows.TableRows, dbOps, inserts);

                var result = new Dictionary<IRemoteTableHandle, HashSet<byte[]>>
                {
                    [table] = inserts
                };

                return (dbOps, result);
            }

            void PreProcessInsertOnlyTable(IRemoteTableHandle table, TableUpdate update, List<DbOp> dbOps, HashSet<byte[]> inserts)
            {
                // In debug mode, make sure we use a byte array comparer in HashSet and not a reference-equal `byte[]` by accident.
                Debug.Assert(inserts.Comparer is ByteArrayComparer);

                foreach (var cqu in update.Updates)
                {
                    var qu = DecompressDecodeQueryUpdate(cqu);
                    if (qu.Deletes.RowsData.Count > 0)
                    {
                        Log.Warn("Non-insert during an insert-only server message!");
                    }
                    foreach (var bin in BsatnRowListIter(qu.Inserts))
                    {
                        if (!inserts.Add(bin))
                        {
                            // Ignore duplicate inserts in the same subscription update.
                            continue;
                        }
                        var obj = table.DecodeValue(bin);
                        var op = new DbOp
                        {
                            table = table,
                            insert = new(obj, bin),
                        };
                        dbOps.Add(op);
                    }
                }
            }


            /// <summary>
            /// TODO: the dictionary is here for backwards compatibility and can be removed
            /// once we get rid of legacy subscriptions.
            /// </summary>
            List<DbOp> PreProcessUnsubscribeApplied(UnsubscribeApplied unsubApplied)
            {
                var table = Db.GetTable(unsubApplied.Rows.TableName) ?? throw new Exception($"Unknown table name: {unsubApplied.Rows.TableName}");
                var dbOps = new List<DbOp>();

                // First apply all of the state
                foreach (var cqu in unsubApplied.Rows.TableRows.Updates)
                {
                    var qu = DecompressDecodeQueryUpdate(cqu);
                    if (qu.Inserts.RowsData.Count > 0)
                    {
                        Log.Warn("Non-insert during an UnsubscribeApplied!");
                    }
                    foreach (var bin in BsatnRowListIter(qu.Deletes))
                    {
                        var obj = table.DecodeValue(bin);
                        var op = new DbOp
                        {
                            table = table,
                            delete = new(obj, bin),
                        };
                        dbOps.Add(op);
                    }
                }

                return dbOps;
            }



            List<DbOp> PreProcessDatabaseUpdate(DatabaseUpdate updates)
            {
                var dbOps = new List<DbOp>();

                // All row updates that have a primary key, this contains inserts, deletes and updates.
                // TODO: is there any guarantee that transaction update contains each table only once, aka updates are already grouped by table?
                // If so, we could simplify this and other methods by moving the dictionary inside the main loop and using only the primary key as key.
                var primaryKeyChanges = new Dictionary<(IRemoteTableHandle table, object primaryKeyValue), DbOp>();

                // First apply all of the state
                foreach (var (table, update) in GetTables(updates))
                {
                    foreach (var cqu in update.Updates)
                    {
                        var qu = DecompressDecodeQueryUpdate(cqu);
                        foreach (var row in BsatnRowListIter(qu.Inserts))
                        {
                            var op = new DbOp { table = table, insert = Decode(table, row, out var pk) };
                            if (pk != null)
                            {
                                // Compound key that we use for lookup.
                                // Consists of the table handle (for faster comparison that string names) + actual primary key of the row.
                                var key = (table, pk);

                                if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                {
                                    if (oldOp.insert is not null)
                                    {
                                        Log.Warn($"Update with the same primary key was applied multiple times! tableName={update.TableName}");
                                        // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                        // SpacetimeDB side.
                                        continue;
                                    }

                                    op.delete = oldOp.delete;
                                }
                                primaryKeyChanges[key] = op;
                            }
                            else
                            {
                                dbOps.Add(op);
                            }
                        }

                        foreach (var row in BsatnRowListIter(qu.Deletes))
                        {
                            var op = new DbOp { table = table, delete = Decode(table, row, out var pk) };
                            if (pk != null)
                            {
                                // Compound key that we use for lookup.
                                // Consists of the table handle (for faster comparison that string names) + actual primary key of the row.
                                var key = (table, pk);

                                if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                {
                                    if (oldOp.delete is not null)
                                    {
                                        Log.Warn($"Update with the same primary key was applied multiple times! tableName={update.TableName}");
                                        // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                        // SpacetimeDB side.
                                        continue;
                                    }

                                    op.insert = oldOp.insert;
                                }
                                primaryKeyChanges[key] = op;
                            }
                            else
                            {
                                dbOps.Add(op);
                            }
                        }
                    }
                }
                // Combine primary key updates and non-primary key updates
                dbOps.AddRange(primaryKeyChanges.Values);
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

            PreProcessedMessage PreProcessMessage(UnprocessedMessage unprocessed)
            {
                var dbOps = new List<DbOp>();

                var message = DecompressDecodeMessage(unprocessed.bytes);

                ReducerEvent<Reducer>? reducerEvent = default;

                // This is all of the inserts, used for updating the stale but un-cleared client cache.
                Dictionary<IRemoteTableHandle, HashSet<byte[]>>? subscriptionInserts = null;

                switch (message)
                {
                    case ServerMessage.InitialSubscription(var initSub):
                        (dbOps, subscriptionInserts) = PreProcessLegacySubscription(initSub);
                        break;
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        (dbOps, subscriptionInserts) = PreProcessSubscribeApplied(subscribeApplied);
                        break;
                    case ServerMessage.SubscriptionError(var subscriptionError):
                        break;
                    case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                        dbOps = PreProcessUnsubscribeApplied(unsubscribeApplied);
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
                                transactionUpdate.CallerAddress,
                                transactionUpdate.EnergyQuantaUsed.Quanta,
                                ToReducer(transactionUpdate));
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
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

                return new PreProcessedMessage
                {
                    processed = new ProcessedMessage { message = message, dbOps = dbOps, timestamp = unprocessed.timestamp, reducerEvent = reducerEvent },
                    subscriptionInserts = subscriptionInserts,
                };
            }
        }

        ProcessedMessage CalculateStateDiff(PreProcessedMessage preProcessedMessage)
        {
            var processed = preProcessedMessage.processed;

            // Perform the state diff, this has to be done on the main thread because we have to touch
            // the client cache.
            if (preProcessedMessage.subscriptionInserts is { } subscriptionInserts)
            {
                foreach (var (table, hashSet) in subscriptionInserts)
                {
                    foreach (var (rowBytes, oldValue) in table.IterEntries().Where(kv => !hashSet.Contains(kv.Key)))
                    {
                        processed.dbOps.Add(new DbOp
                        {
                            table = table,
                            // This is a row that we had before, but we do not have it now.
                            // This must have been a delete.
                            delete = new(oldValue, rowBytes),
                        });
                    }
                }
            }

            return processed;
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

        private void OnMessageProcessCompleteUpdate(IEventContext eventContext, List<DbOp> dbOps)
        {
            // First trigger OnBeforeDelete
            foreach (var update in dbOps)
            {
                if (update is { delete: { value: var oldValue }, insert: null })
                {
                    try
                    {
                        update.table.InvokeBeforeDelete(eventContext, oldValue);
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                }
            }

            // Apply all of the state
            for (var i = 0; i < dbOps.Count; i++)
            {
                // TODO: Reimplement updates when we add support for primary keys
                var update = dbOps[i];

                if (update.delete is { } delete)
                {
                    if (!update.table.DeleteEntry(delete.bytes))
                    {
                        update.delete = null;
                        dbOps[i] = update;
                    }
                }

                if (update.insert is { } insert)
                {
                    if (!update.table.InsertEntry(insert.bytes, insert.value))
                    {
                        update.insert = null;
                        dbOps[i] = update;
                    }
                }
            }

            // Send out events
            foreach (var dbOp in dbOps)
            {
                try
                {
                    switch (dbOp)
                    {
                        case { insert: { value: var newValue }, delete: { value: var oldValue } }:
                            dbOp.table.InvokeUpdate(eventContext, oldValue, newValue);
                            break;

                        case { insert: { value: var newValue } }:
                            dbOp.table.InvokeInsert(eventContext, newValue);
                            break;

                        case { delete: { value: var oldValue } }:
                            dbOp.table.InvokeDelete(eventContext, oldValue);
                            break;
                    }
                }
                catch (Exception e)
                {
                    Log.Exception(e);
                }
            }
        }

        protected abstract bool Dispatch(IReducerEventContext context, Reducer reducer);

        private void OnMessageProcessComplete(PreProcessedMessage preProcessed)
        {
            var processed = CalculateStateDiff(preProcessed);
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
                    {
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.SubscribeApplied)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(subscribeApplied.RequestId);
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
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
                    {
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.UnsubscribeApplied)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(unsubscribeApplied.RequestId);
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.UnsubscribeApplied());
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);
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

                        if (processed.reducerEvent is not { } reducerEvent)
                        {
                            // If we are here, an error about unknown reducer should have already been logged, so nothing to do.
                            break;
                        }

                        var eventContext = ToReducerEventContext(reducerEvent);
                        var legacyEventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                        OnMessageProcessCompleteUpdate(legacyEventContext, dbOps);

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

            webSocket.Send(new ClientMessage.Unsubscribe(new()
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
}
