using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net.WebSockets;
using System.Threading;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;
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
            conn.Connect(token, uri, nameOrAddress, compression ?? Compression.Brotli);
#if UNITY_5_3_OR_NEWER
            SpacetimeDBNetworkManager.ActiveConnections.Add(conn);
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

        public DbConnectionBuilder<DbConnection, Reducer> WithCredentials(in (Identity identity, string token)? creds)
        {
            token = creds?.token;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithCompression(Compression compression)
        {
            this.compression = compression;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> OnConnect(Action<DbConnection, Identity, string> cb)
        {
            conn.onConnect += (identity, token) => cb.Invoke(conn, identity, token);
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> OnConnectError(Action<WebSocketError?, string> cb)
        {
            conn.webSocket.OnConnectError += (a, b) => cb.Invoke(a, b);
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> OnDisconnect(Action<DbConnection, WebSocketCloseStatus?, WebSocketError?> cb)
        {
            conn.webSocket.OnClose += (code, error) => cb.Invoke(conn, code, error);
            return this;
        }
    }

    public interface IDbConnection
    {
        internal void Subscribe(ISubscriptionHandle handle, string[] querySqls);
        void FrameTick();
        void Disconnect();
    }

    public abstract class DbConnectionBase<DbConnection, Reducer> : IDbConnection
        where DbConnection : DbConnectionBase<DbConnection, Reducer>, new()
    {
        public static DbConnectionBuilder<DbConnection, Reducer> Builder() => new();

        readonly struct DbValue
        {
            public readonly IDatabaseRow value;
            public readonly byte[] bytes;

            public DbValue(IDatabaseRow value, byte[] bytes)
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

        private readonly Dictionary<uint, ISubscriptionHandle> subscriptions = new();

        /// <summary>
        /// Invoked when a reducer is returned with an error and has no client-side handler.
        /// </summary>
        [Obsolete]
        public event Action<ReducerEvent<Reducer>>? onUnhandledReducerError;

        public readonly Address Address = Address.Random();
        public Identity? Identity { get; private set; }

        internal WebSocket webSocket;
        private bool connectionClosed;
        protected readonly ClientCache clientDB = new();

        protected abstract Reducer ToReducer(TransactionUpdate update);
        protected abstract IEventContext ToEventContext(Event<Reducer> reducerEvent);

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
            webSocket.OnClose += (a, b) => SpacetimeDBNetworkManager.ActiveConnections.Remove(this);
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
            public Dictionary<Type, HashSet<byte[]>>? subscriptionInserts;
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

        private static BinaryReader BrotliReader(Stream stream)
        {
            return new BinaryReader(new BrotliStream(stream, CompressionMode.Decompress));
        }

        private static BinaryReader GzipReader(Stream stream)
        {
            return new BinaryReader(new GZipStream(stream, CompressionMode.Decompress));
        }

        private static ServerMessage DecompressDecodeMessage(byte[] bytes)
        {
            using var stream = new MemoryStream(bytes, 1, bytes.Length - 1);

            // The stream will never be empty. It will at least contain the compression algo.
            var compression = (CompressionAlgos)bytes[0];
            // Conditionally decompress and decode.
            switch (compression)
            {
                case CompressionAlgos.None:
                    return new ServerMessage.BSATN().Read(new BinaryReader(stream));
                case CompressionAlgos.Brotli:
                    return new ServerMessage.BSATN().Read(BrotliReader(stream));
                case CompressionAlgos.Gzip:
                    return new ServerMessage.BSATN().Read(GzipReader(stream));
                default:
                    throw new InvalidOperationException("Unknown compression type");
            }
        }

        private static QueryUpdate DecompressDecodeQueryUpdate(CompressableQueryUpdate update)
        {
            switch (update)
            {
                case CompressableQueryUpdate.Uncompressed(var qu):
                    return qu;

                case CompressableQueryUpdate.Brotli(var bytes):
                    return new QueryUpdate.BSATN().Read(BrotliReader(new MemoryStream(bytes)));

                case CompressableQueryUpdate.Gzip(var bytes):
                    return new QueryUpdate.BSATN().Read(GzipReader(new MemoryStream(bytes)));

                default:
                    throw new InvalidOperationException();
            }
        }

        private static int BsatnRowListCount(BsatnRowList list)
        {
            switch (list.SizeHint)
            {
                case RowSizeHint.FixedSize(var size):
                    return list.RowsData.Length / size;
                case RowSizeHint.RowOffsets(var offsets):
                    return offsets.Count;
                default:
                    throw new InvalidOperationException("Unknown RowSizeHint variant");
            }
        }

        private static IEnumerable<byte[]> BsatnRowListIter(BsatnRowList list)
        {
            var count = BsatnRowListCount(list);
            for (int index = 0; index < count; index += 1)
            {
                switch (list.SizeHint)
                {
                    case RowSizeHint.FixedSize(var size):
                        {
                            int start = index * size;
                            int elemLen = size;
                            yield return new ReadOnlySpan<byte>(list.RowsData, start, elemLen).ToArray();
                            break;
                        }
                    case RowSizeHint.RowOffsets(var offsets):
                        {
                            int start = (int)offsets[index];
                            // The end is either the start of the next element or the end.
                            int end;
                            if (index + 1 == count)
                            {
                                end = list.RowsData.Length;
                            }
                            else
                            {
                                end = (int)offsets[index + 1];
                            }
                            int elemLen = end - start;
                            yield return new ReadOnlyMemory<byte>(list.RowsData, start, elemLen).ToArray();
                            break;
                        }
                }
            }
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

            PreProcessedMessage PreProcessMessage(UnprocessedMessage unprocessed)
            {
                var dbOps = new List<DbOp>();

                var message = DecompressDecodeMessage(unprocessed.bytes);

                ReducerEvent<Reducer>? reducerEvent = default;

                // This is all of the inserts
                Dictionary<System.Type, HashSet<byte[]>>? subscriptionInserts = null;
                // All row updates that have a primary key, this contains inserts, deletes and updates
                var primaryKeyChanges = new Dictionary<(System.Type tableType, object primaryKeyValue), DbOp>();

                HashSet<byte[]> GetInsertHashSet(System.Type tableType, int tableSize)
                {
                    if (!subscriptionInserts.TryGetValue(tableType, out var hashSet))
                    {
                        hashSet = new HashSet<byte[]>(capacity: tableSize, comparer: ByteArrayComparer.Instance);
                        subscriptionInserts[tableType] = hashSet;
                    }

                    return hashSet;
                }

                switch (message)
                {
                    case ServerMessage.InitialSubscription(var initialSubscription):
                        int cap = initialSubscription.DatabaseUpdate.Tables.Sum(a => (int)a.NumRows);
                        subscriptionInserts = new(capacity: cap);

                        // First apply all of the state
                        foreach (var update in initialSubscription.DatabaseUpdate.Tables)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                Log.Error($"Unknown table name: {tableName}");
                                continue;
                            }

                            var hashSet = GetInsertHashSet(table.ClientTableType, (int)update.NumRows);

                            foreach (var cqu in update.Updates)
                            {
                                var qu = DecompressDecodeQueryUpdate(cqu);
                                if (BsatnRowListCount(qu.Deletes) != 0)
                                {
                                    Log.Warn("Non-insert during a subscription update!");
                                }

                                foreach (var bin in BsatnRowListIter(qu.Inserts))
                                {
                                    if (!hashSet.Add(bin))
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
                        break;

                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        // Convert the generic event arguments in to a domain specific event object
                        try
                        {
                            reducerEvent = new(
                                DateTimeOffset.FromUnixTimeMilliseconds((long)transactionUpdate.Timestamp.Microseconds / 1000),
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
                            primaryKeyChanges = new();

                            // First apply all of the state
                            foreach (var update in committed.Tables)
                            {
                                var tableName = update.TableName;
                                var table = clientDB.GetTable(tableName);
                                if (table == null)
                                {
                                    Log.Error($"Unknown table name: {tableName}");
                                    continue;
                                }

                                foreach (var cqu in update.Updates)
                                {
                                    var qu = DecompressDecodeQueryUpdate(cqu);
                                    foreach (var row in BsatnRowListIter(qu.Inserts))
                                    {
                                        var op = new DbOp { table = table, insert = Decode(table, row, out var pk) };
                                        if (pk != null)
                                        {
                                            // Compound key that we use for lookup.
                                            // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                            var key = (table.ClientTableType, pk);

                                            if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                            {
                                                if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                                {
                                                    Log.Warn($"Update with the same primary key was applied multiple times! tableName={tableName}");
                                                    // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                                    // SpacetimeDB side.
                                                    continue;
                                                }

                                                var (insertOp, deleteOp) = op.insert is not null ? (op, oldOp) : (oldOp, op);
                                                op = new DbOp
                                                {
                                                    table = insertOp.table,
                                                    delete = deleteOp.delete,
                                                    insert = insertOp.insert,
                                                };
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
                                            // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                            var key = (table.ClientTableType, pk);

                                            if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                            {
                                                if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                                {
                                                    Log.Warn($"Update with the same primary key was applied multiple times! tableName={tableName}");
                                                    // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                                    // SpacetimeDB side.
                                                    continue;
                                                }

                                                var (insertOp, deleteOp) = op.insert is not null ? (op, oldOp) : (oldOp, op);
                                                op = new DbOp
                                                {
                                                    table = insertOp.table,
                                                    delete = deleteOp.delete,
                                                    insert = insertOp.insert,
                                                };
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
                        }
                        break;
                    case ServerMessage.IdentityToken(var identityToken):
                        break;
                    case ServerMessage.OneOffQueryResponse(var resp):
                        /// This case does NOT produce a list of DBOps, because it should not modify the client cache state!
                        var messageId = new Guid(resp.MessageId);

                        if (!waitingOneOffQueries.Remove(messageId, out var resultSource))
                        {
                            Log.Error($"Response to unknown one-off-query: {messageId}");
                            break;
                        }

                        resultSource.SetResult(resp);
                        break;
                    default:
                        throw new InvalidOperationException();
                }

                // Logger.LogWarning($"Total Updates preprocessed: {totalUpdateCount}");
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
                foreach (var table in clientDB.GetTables())
                {
                    if (!subscriptionInserts.TryGetValue(table.ClientTableType, out var hashSet))
                    {
                        continue;
                    }

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
        internal void Connect(string? token, string uri, string addressOrName, Compression compression)
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
                        await webSocket.Connect(token, uri, addressOrName, Address, compression);
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
                    if (update.table.DeleteEntry(delete.bytes))
                    {
                        update.table.InternalInvokeValueDeleted(delete.value);
                    }
                    else
                    {
                        update.delete = null;
                        dbOps[i] = update;
                    }
                }

                if (update.insert is { } insert)
                {
                    if (update.table.InsertEntry(insert.bytes, insert.value))
                    {
                        update.table.InternalInvokeValueInserted(insert.value);
                    }
                    else
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

        protected abstract bool Dispatch(IEventContext context, Reducer reducer);

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
                        var eventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
                        if (subscriptions.TryGetValue(initialSubscription.RequestId, out var subscription))
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
        public void InternalCallReducer<T>(T args)
            where T : IReducerArgs, new()
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call reducer, not connected to server!");
                return;
            }

            webSocket.Send(new ClientMessage.CallReducer(
                new CallReducer
                {
                    RequestId = stats.ReducerRequestTracker.StartTrackingRequest(args.ReducerName),
                    Reducer = args.ReducerName,
                    Args = IStructuralReadWrite.ToBytes(args)
                }
            ));
        }

        void IDbConnection.Subscribe(ISubscriptionHandle handle, string[] querySqls)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return;
            }

            var id = stats.SubscriptionRequestTracker.StartTrackingRequest();
            subscriptions[id] = handle;
            webSocket.Send(new ClientMessage.Subscribe(
                new Subscribe
                {
                    RequestId = id,
                    QueryStrings = querySqls.ToList()
                }
            ));
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("WHERE sender = \"bob\"");
        public async Task<T[]> OneOffQuery<T>(string query)
            where T : IDatabaseRow, new()
        {
            var messageId = Guid.NewGuid();
            var type = typeof(T);
            var resultSource = new TaskCompletionSource<OneOffQueryResponse>();
            waitingOneOffQueries[messageId] = resultSource;

            // unsanitized here, but writes will be prevented serverside.
            // the best they can do is send multiple selects, which will just result in them getting no data back.
            string queryString = $"SELECT * FROM {type.Name} {query}";

            var requestId = stats.OneOffRequestTracker.StartTrackingRequest();
            webSocket.Send(new ClientMessage.OneOffQuery(new OneOffQuery
            {
                MessageId = messageId.ToByteArray(),
                QueryString = queryString,
            }));

            // Suspend for an arbitrary amount of time
            var result = await resultSource.Task;

            if (!stats.OneOffRequestTracker.FinishTrackingRequest(requestId))
            {
                Log.Warn($"Failed to finish tracking one off request: {requestId}");
            }

            T[] LogAndThrow(string error)
            {
                error = $"While processing one-off-query `{queryString}`, ID {messageId}: {error}";
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

            if (cacheTable?.ClientTableType != type)
            {
                return LogAndThrow($"Mismatched result type, expected {type} but got {resultTable.TableName}");
            }

            return BsatnRowListIter(resultTable.Rows)
                .Select(row => BSATNHelpers.Decode<T>(row))
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
    }
}
