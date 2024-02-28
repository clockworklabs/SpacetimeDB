using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.WebSockets;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Threading.Channels;
using System.Threading.Tasks;
using ClientApi;
using Newtonsoft.Json;
using SpacetimeDB.SATS;
using Channel = System.Threading.Channels.Channel;
using Thread = System.Threading.Thread;

namespace SpacetimeDB
{
    public class SpacetimeDBClient
    {
        public enum TableOp
        {
            Insert,
            Delete,
            Update,
            NoChange,
        }

        public class ReducerCallRequest
        {
            public string fn;
            public object[] args;
        }

        public class SubscriptionRequest
        {
            public string subscriptionQuery;
        }

        public struct DbOp
        {
            public ClientCache.TableCache table;
            public TableOp op;
            public object newValue;
            public object oldValue;
            public byte[] deletedBytes;
            public byte[] insertedBytes;
            public AlgebraicValue rowValue;
            public AlgebraicValue primaryKeyValue;
        }

        public delegate void RowUpdate(string tableName, TableOp op, object oldValue, object newValue,
            SpacetimeDB.ReducerEventBase dbEvent);

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?, string> onConnectError;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception> onSendError;

        /// <summary>
        /// Called when a connection that was established has disconnected.
        /// </summary>
        public event Action<WebSocketCloseStatus?, WebSocketError?> onDisconnect;

        /// <summary>
        /// Invoked on each row update to each table.
        /// </summary>
        public event RowUpdate onRowUpdate;

        /// <summary>
        /// Invoked when a subscription is about to start being processed. This is called even before OnBeforeDelete.
        /// </summary>
        public event Action onBeforeSubscriptionApplied;
        
        /// <summary>
        /// Invoked when the local client cache is updated as a result of changes made to the subscription queries.
        /// </summary>
        public event Action onSubscriptionApplied;

        /// <summary>
        /// Invoked when a reducer is returned with an error and has no client-side handler.
        /// </summary>
        public event Action<ReducerEventBase> onUnhandledReducerError;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<string, Identity, Address> onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ClientApi.Event> onEvent;

        public Address clientAddress { get; private set; }

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        public static ClientCache clientDB;

        public static Dictionary<string, Func<ClientApi.Event, bool>> reducerEventCache =
            new Dictionary<string, Func<ClientApi.Event, bool>>();

        public static Dictionary<string, Action<ClientApi.Event>> deserializeEventCache =
            new Dictionary<string, Action<ClientApi.Event>>();

        private static Dictionary<Guid, Channel<OneOffQueryResponse>> waitingOneOffQueries =
            new Dictionary<Guid, Channel<OneOffQueryResponse>>();

        private bool isClosing;
        private Thread networkMessageProcessThread;
        private Thread stateDiffProcessThread;

        public static SpacetimeDBClient instance;

        public ISpacetimeDBLogger Logger => logger;
        private ISpacetimeDBLogger logger;

        public static void CreateInstance(ISpacetimeDBLogger loggerToUse)
        {
            if (instance == null)
            {
                new SpacetimeDBClient(loggerToUse);
            }
            else
            {
                loggerToUse.LogError($"Instance already created.");
            }
        }

        public Type FindReducerType()
        {
            // Get all loaded assemblies
            Assembly[] assemblies = AppDomain.CurrentDomain.GetAssemblies();

            // Iterate over each assembly and search for the type
            foreach (Assembly assembly in assemblies)
            {
                // Get all types in the assembly
                Type[] types = assembly.GetTypes();

                // Search for the class with the attribute ReducerClass
                foreach (Type type in types)
                {
                    if (type.GetCustomAttribute<ReducerClassAttribute>() != null)
                    {
                        return type;
                    }
                }
            }

            // If the type is not found in any assembly, return null or throw an exception
            return null;
        }

        protected SpacetimeDBClient(ISpacetimeDBLogger loggerToUse)
        {
            if (instance != null)
            {
                loggerToUse.LogError($"There is more than one {GetType()}");
                return;
            }

            instance = this;

            clientAddress = Address.Random();

            logger = loggerToUse;

            var options = new SpacetimeDB.ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new SpacetimeDB.WebSocket(logger, options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnClose += (code, error) => onDisconnect?.Invoke(code, error);
            webSocket.OnConnect += () => onConnect?.Invoke();
            webSocket.OnConnectError += (a, b) => onConnectError?.Invoke(a, b);
            webSocket.OnSendError += a => onSendError?.Invoke(a);

            clientDB = new ClientCache();

            var type = typeof(IDatabaseTable);
            var types = AppDomain.CurrentDomain.GetAssemblies().SelectMany(s => s.GetTypes())
                .Where(p => type.IsAssignableFrom(p));
            foreach (var @class in types)
            {
                if (!@class.IsClass)
                {
                    continue;
                }

                var algebraicTypeFunc = @class.GetMethod("GetAlgebraicType", BindingFlags.Static | BindingFlags.Public);
                var algebraicValue = algebraicTypeFunc!.Invoke(null, null) as AlgebraicType;
                var conversionFunc = @class.GetMethods()
                    .FirstOrDefault(a => a.Name == "op_Explicit" &&
                                         a.GetParameters().Length > 0 &&
                                         a.GetParameters()[0].ParameterType ==
                                         typeof(AlgebraicValue));
                clientDB.AddTable(@class, algebraicValue,
                    a => { return conversionFunc!.Invoke(null, new object[] { a }); });
            }

            var reducerType = FindReducerType();
            if (reducerType != null)
            {
                // cache all our reducer events by their function name 
                foreach (var methodInfo in reducerType.GetMethods())
                {
                    if (methodInfo.GetCustomAttribute<ReducerCallbackAttribute>() is
                        { } reducerEvent)
                    {
                        reducerEventCache.Add(reducerEvent.FunctionName,
                            (Func<ClientApi.Event, bool>)methodInfo.CreateDelegate(
                                typeof(Func<ClientApi.Event, bool>)));
                    }

                    if (methodInfo.GetCustomAttribute<DeserializeEventAttribute>() is
                        { } deserializeEvent)
                    {
                        deserializeEventCache.Add(deserializeEvent.FunctionName,
                            (Action<ClientApi.Event>)methodInfo.CreateDelegate(typeof(Action<ClientApi.Event>)));
                    }
                }
            }
            else
            {
                loggerToUse.LogError($"Could not find reducer type. Have you run spacetime generate?");
            }

            _preProcessCancellationToken = _preProcessCancellationTokenSource.Token;
            networkMessageProcessThread = new Thread(PreProcessMessages);
            networkMessageProcessThread.Start();

            _stateDiffCancellationToken = _stateDiffCancellationTokenSource.Token;
            stateDiffProcessThread = new Thread(ExecuteStateDiff);
            stateDiffProcessThread.Start();
        }

        struct PreProcessedMessage
        {
            public Message message;
            public List<DbOp> dbOps;
            public Dictionary<string, HashSet<byte[]>> inserts;
        }

        private readonly BlockingCollection<byte[]> _messageQueue =
            new BlockingCollection<byte[]>(new ConcurrentQueue<byte[]>());

        private readonly BlockingCollection<PreProcessedMessage> _preProcessedNetworkMessages =
            new BlockingCollection<PreProcessedMessage>(new ConcurrentQueue<PreProcessedMessage>());

        private CancellationTokenSource _preProcessCancellationTokenSource = new CancellationTokenSource();
        private CancellationToken _preProcessCancellationToken;

        void PreProcessMessages()
        {
            while (!isClosing)
            {
                try
                {
                    var bytes = _messageQueue.Take(_preProcessCancellationToken);
                    var preprocessedMessage = PreProcessMessage(bytes);
                    _preProcessedNetworkMessages.Add(preprocessedMessage, _preProcessCancellationToken);
                }
                catch (OperationCanceledException)
                {
                    // Normal shutdown
                    return;
                }
            }

            PreProcessedMessage PreProcessMessage(byte[] bytes)
            {
                var dbOps = new List<DbOp>();
                var message = Message.Parser.ParseFrom(bytes);
                using var stream = new MemoryStream();
                using var reader = new BinaryReader(stream);

                // This is all of the inserts
                Dictionary<string, HashSet<byte[]>> subscriptionInserts = null;
                // All row updates that have a primary key, this contains inserts, deletes and updates
                var primaryKeyChanges = new Dictionary<string, Dictionary<AlgebraicValue, DbOp>>();

                Dictionary<AlgebraicValue, DbOp> GetPrimaryKeyLookup(string tableName, AlgebraicType schema)
                {
                    if (!primaryKeyChanges.TryGetValue(tableName, out var value))
                    {
                        value = new Dictionary<AlgebraicValue, DbOp>(new AlgebraicValue.AlgebraicValueComparer(schema));
                        primaryKeyChanges[tableName] = value;
                    }

                    return value;
                }

                HashSet<byte[]> GetInsertHashSet(string tableName, int tableSize)
                {
                    if (!subscriptionInserts.TryGetValue(tableName, out var hashSet))
                    {
                        hashSet = new HashSet<byte[]>(capacity:tableSize, comparer: new ClientCache.TableCache.ByteArrayComparer());
                        subscriptionInserts[tableName] = hashSet;
                    }

                    return hashSet;
                }

                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        subscriptionInserts = new Dictionary<string, HashSet<byte[]>>(
                            capacity: subscriptionUpdate.TableUpdates.Sum(a => a.TableRowOperations.Count));
                        // First apply all of the state
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            var tableName = update.TableName;
                            var hashSet = GetInsertHashSet(tableName, subscriptionUpdate.TableUpdates.Count);
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                logger.LogError($"Unknown table name: {tableName}");
                                continue;
                            }

                            foreach (var row in update.TableRowOperations)
                            {
                                var rowBytes = row.Row.ToByteArray();
                                stream.Position = 0;
                                stream.Write(rowBytes, 0, rowBytes.Length);
                                stream.Position = 0;
                                stream.SetLength(rowBytes.Length);
                                var deserializedRow = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                if (deserializedRow == null)
                                {
                                    throw new Exception("Failed to deserialize row");
                                }

                                if (row.Op != TableRowOperation.Types.OperationType.Insert)
                                {
                                    logger.LogWarning("Non-insert during a subscription update!");
                                    continue;
                                }

                                table.SetAndForgetDecodedValue(deserializedRow, out var obj);
                                var op = new DbOp
                                {
                                    table = table,
                                    deletedBytes = null,
                                    insertedBytes = rowBytes,
                                    op = TableOp.Insert,
                                    newValue = obj,
                                    oldValue = null,
                                    primaryKeyValue = null,
                                    rowValue = deserializedRow,
                                };

                                if (!hashSet.Add(rowBytes))
                                {
                                    logger.LogError($"Multiple of the same insert in the same subscription update: table={table.Name} rowBytes={rowBytes}");
                                }
                                else
                                {
                                    dbOps.Add(op);
                                }
                            }
                        }

                        break;

                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        // First apply all of the state
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                logger.LogError($"Unknown table name: {tableName}");
                                continue;
                            }

                            foreach (var row in update.TableRowOperations)
                            {
                                var rowBytes = row.Row.ToByteArray();
                                stream.Position = 0;
                                stream.Write(rowBytes, 0, rowBytes.Length);
                                stream.Position = 0;
                                stream.SetLength(rowBytes.Length);
                                var deserializedRow = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                if (deserializedRow == null)
                                {
                                    throw new Exception("Failed to deserialize row");
                                }

                                var primaryKeyValue = table.GetPrimaryKeyValue(deserializedRow);
                                var primaryKeyType = table.GetPrimaryKeyType();
                                table.SetAndForgetDecodedValue(deserializedRow, out var obj);

                                var op = new DbOp
                                {
                                    table = table,
                                    deletedBytes =
                                        row.Op == TableRowOperation.Types.OperationType.Delete ? rowBytes : null,
                                    insertedBytes =
                                        row.Op == TableRowOperation.Types.OperationType.Delete ? null : rowBytes,
                                    op = row.Op == TableRowOperation.Types.OperationType.Delete
                                        ? TableOp.Delete
                                        : TableOp.Insert,
                                    newValue = row.Op == TableRowOperation.Types.OperationType.Delete ? null : obj,
                                    oldValue = row.Op == TableRowOperation.Types.OperationType.Delete ? obj : null,
                                    primaryKeyValue = primaryKeyValue,
                                    rowValue = deserializedRow,
                                };

                                if (primaryKeyType != null)
                                {
                                    var primaryKeyLookup = GetPrimaryKeyLookup(tableName, primaryKeyType);
                                    if (primaryKeyLookup.TryGetValue(primaryKeyValue, out var value))
                                    {
                                        if (value.op == op.op || value.op == TableOp.Update)
                                        {
                                            logger.LogWarning($"Update with the same primary key was " +
                                                              $"applied multiple times! tableName={tableName}");
                                            // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                            // SpacetimeDB side.
                                            continue;
                                        }

                                        var insertOp = op;
                                        var deleteOp = value;
                                        if (op.op == TableOp.Delete)
                                        {
                                            insertOp = value;
                                            deleteOp = op;
                                        }

                                        primaryKeyLookup[primaryKeyValue] = new DbOp
                                        {
                                            table = insertOp.table,
                                            op = TableOp.Update,
                                            newValue = insertOp.newValue,
                                            oldValue = deleteOp.oldValue,
                                            deletedBytes = deleteOp.deletedBytes,
                                            insertedBytes = insertOp.insertedBytes,
                                            primaryKeyValue = insertOp.primaryKeyValue,
                                            rowValue = insertOp.rowValue,
                                        };
                                    }
                                    else
                                    {
                                        primaryKeyLookup[primaryKeyValue] = op;
                                    }
                                }
                                else
                                {
                                    dbOps.Add(op);
                                }
                            }
                        }

                        // Combine primary key updates and non-primary key updates
                        dbOps.AddRange(primaryKeyChanges.Values.SelectMany(a => a.Values));

                        // Convert the generic event arguments in to a domain specific event object, this gets fed back into
                        // the message.TransactionUpdate.Event.FunctionCall.CallInfo field.
                        if (message.TypeCase == Message.TypeOneofCase.TransactionUpdate &&
                            deserializeEventCache.TryGetValue(message.TransactionUpdate.Event.FunctionCall.Reducer,
                                out var deserializer))
                        {
                            deserializer.Invoke(message.TransactionUpdate.Event);
                        }

                        break;
                    case ClientApi.Message.TypeOneofCase.IdentityToken:
                        break;
                    case ClientApi.Message.TypeOneofCase.Event:
                        break;
                    case ClientApi.Message.TypeOneofCase.OneOffQuery:
                        break;
                    case ClientApi.Message.TypeOneofCase.OneOffQueryResponse:
                        /// This case does NOT produce a list of DBOps, because it should not modify the client cache state!
                        var resp = message.OneOffQueryResponse;
                        Guid messageId = new Guid(resp.MessageId.Span);

                        if (!waitingOneOffQueries.ContainsKey(messageId))
                        {
                            logger.LogError("Response to unknown one-off-query: " + messageId);
                            break;
                        }

                        waitingOneOffQueries[messageId].Writer.TryWrite(resp);
                        waitingOneOffQueries.Remove(messageId);
                        break;
                }


                // logger.LogWarning($"Total Updates preprocessed: {totalUpdateCount}");
                return new PreProcessedMessage { message = message, dbOps = dbOps, inserts = subscriptionInserts };
            }
        }

        struct ProcessedMessage
        {
            public Message message;
            public List<DbOp> dbOps;
            public HashSet<byte[]> inserts;
        }

        // The message that has been preprocessed and has had its state diff calculated
        
        private BlockingCollection<ProcessedMessage> _stateDiffMessages = new BlockingCollection<ProcessedMessage>();
        private CancellationTokenSource _stateDiffCancellationTokenSource = new CancellationTokenSource();
        private CancellationToken _stateDiffCancellationToken;

        void ExecuteStateDiff()
        {
            while (!isClosing)
            {
                try
                {
                    var message = _preProcessedNetworkMessages.Take(_stateDiffCancellationToken);
                    var (m, events) = CalculateStateDiff(message);
                    _stateDiffMessages.Add(new ProcessedMessage { dbOps = events, message = m, });
                }
                catch (OperationCanceledException)
                {
                    // Normal shutdown
                    return;
                }
            }

            (Message, List<DbOp>) CalculateStateDiff(PreProcessedMessage preProcessedMessage)
            {
                var message = preProcessedMessage.message;
                var dbOps = preProcessedMessage.dbOps;
                // Perform the state diff, this has to be done on the main thread because we have to touch
                // the client cache.
                if (message.TypeCase == Message.TypeOneofCase.SubscriptionUpdate)
                {
                    foreach (var table in clientDB.GetTables())
                    {
                        foreach (var rowBytes in table.entries.Keys)
                        {
                            if (!preProcessedMessage.inserts.TryGetValue(table.Name, out var hashSet))
                            {
                                continue;
                            }
                            
                            if (!hashSet.Contains(rowBytes))
                            {
                                // This is a row that we had before, but we do not have it now.
                                // This must have been a delete.
                                dbOps.Add(new DbOp
                                {
                                    table = table,
                                    op = TableOp.Delete,
                                    newValue = null,
                                    oldValue = table.entries[rowBytes].Item2,
                                    deletedBytes = rowBytes,
                                    insertedBytes = null,
                                    primaryKeyValue = null
                                });
                            }
                        }
                    }
                }

                return (message, dbOps);
            }
        }

        public void Close()
        {
            isClosing = true;
            connectionClosed = true;
            webSocket.Close();
            _preProcessCancellationTokenSource.Cancel();
            _stateDiffCancellationTokenSource.Cancel();

            webSocket = null;
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="uri"> URI of the SpacetimeDB server (ex: https://testnet.spacetimedb.com)
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        public void Connect(string token, string uri, string addressOrName)
        {
            isClosing = false;

            uri = uri.Replace("http://", "ws://");
            uri = uri.Replace("https://", "wss://");
            if (!uri.StartsWith("ws://") && !uri.StartsWith("wss://"))
            {
                uri = $"ws://{uri}";
            }

            logger.Log($"SpacetimeDBClient: Connecting to {uri} {addressOrName}");
            Task.Run(async () =>
            {
                try
                {
                    await webSocket.Connect(token, uri, addressOrName, clientAddress);
                }
                catch (Exception e)
                {
                    if (connectionClosed)
                    {
                        logger.Log("Connection closed gracefully.");
                        return;
                    }

                    logger.LogException(e);
                }
            });
        }

        private void OnMessageProcessComplete(Message message, List<DbOp> dbOps)
        {
            switch (message.TypeCase)
            {
                case Message.TypeOneofCase.SubscriptionUpdate:
                    onBeforeSubscriptionApplied?.Invoke();
                    break;
            }
            
            switch (message.TypeCase)
            {
                case Message.TypeOneofCase.SubscriptionUpdate:
                case Message.TypeOneofCase.TransactionUpdate:
                    // First trigger OnBeforeDelete
                    foreach (var update in dbOps)
                    {
                        if (update.op == TableOp.Delete)
                        {
                            try
                            {
                                update.table.BeforeDeleteCallback?.Invoke(update.oldValue,
                                    message.TransactionUpdate?.Event);
                            }
                            catch (Exception e)
                            {
                                logger.LogException(e);
                            }
                        }
                    }

                    void InternalDeleteCallback(DbOp op)
                    {
                        if (op.oldValue != null)
                        {
                            op.table.InternalValueDeletedCallback(op.oldValue);
                        }
                        else
                        {
                            logger.LogError("Delete issued, but no value was present!");
                        }
                    }

                    void InternalInsertCallback(DbOp op)
                    {
                        if (op.newValue != null)
                        {
                            op.table.InternalValueInsertedCallback(op.newValue);
                        }
                        else
                        {
                            logger.LogError("Insert issued, but no value was present!");
                        }
                    }

                    // Apply all of the state
                    for (var i = 0; i < dbOps.Count; i++)
                    {
                        // TODO: Reimplement updates when we add support for primary keys
                        var update = dbOps[i];
                        switch (update.op)
                        {
                            case TableOp.Delete:
                                if (dbOps[i].table.DeleteEntry(update.deletedBytes))
                                {
                                    InternalDeleteCallback(update);
                                }
                                else
                                {
                                    var op = dbOps[i];
                                    op.op = TableOp.NoChange;
                                    dbOps[i] = op;
                                }
                                break;
                            case TableOp.Insert:
                                if (dbOps[i].table.InsertEntry(update.insertedBytes, update.rowValue))
                                {
                                    InternalInsertCallback(update);
                                }
                                else
                                {
                                    var op = dbOps[i];
                                    op.op = TableOp.NoChange;
                                    dbOps[i] = op;
                                }
                                break;
                            case TableOp.Update:
                                if (dbOps[i].table.DeleteEntry(update.deletedBytes))
                                {
                                    InternalDeleteCallback(update);
                                }
                                else
                                {
                                    var op = dbOps[i];
                                    op.op = TableOp.NoChange;
                                    dbOps[i] = op;
                                }
                                
                                if (dbOps[i].table.InsertEntry(update.insertedBytes, update.rowValue))
                                {
                                    InternalInsertCallback(update);
                                }
                                else
                                {
                                    var op = dbOps[i];
                                    op.op = TableOp.NoChange;
                                    dbOps[i] = op;
                                }
                                break;
                            default:
                                throw new ArgumentOutOfRangeException();
                        }
                    }

                    // Send out events
                    var updateCount = dbOps.Count;
                    for (var i = 0; i < updateCount; i++)
                    {
                        var tableName = dbOps[i].table.ClientTableType.Name;
                        var tableOp = dbOps[i].op;
                        var oldValue = dbOps[i].oldValue;
                        var newValue = dbOps[i].newValue;

                        switch (tableOp)
                        {
                            case TableOp.Insert:
                                if (oldValue == null && newValue != null)
                                {
                                    try
                                    {
                                        if (dbOps[i].table.InsertCallback != null)
                                        {
                                            dbOps[i].table.InsertCallback.Invoke(newValue,
                                                message.TransactionUpdate?.Event);
                                        }
                                    }
                                    catch (Exception e)
                                    {
                                        logger.LogException(e);
                                    }

                                    try
                                    {
                                        if (dbOps[i].table.RowUpdatedCallback != null)
                                        {
                                            dbOps[i].table.RowUpdatedCallback
                                                .Invoke(tableOp, null, newValue, message.TransactionUpdate?.Event);
                                        }
                                    }
                                    catch (Exception e)
                                    {
                                        logger.LogException(e);
                                    }
                                }
                                else
                                {
                                    logger.LogError("Failed to send callback: invalid insert!");
                                }

                                break;
                            case TableOp.Delete:
                                {
                                    if (oldValue != null && newValue == null)
                                    {
                                        if (dbOps[i].table.DeleteCallback != null)
                                        {
                                            try
                                            {
                                                dbOps[i].table.DeleteCallback.Invoke(oldValue,
                                                    message.TransactionUpdate?.Event);
                                            }
                                            catch (Exception e)
                                            {
                                                logger.LogException(e);
                                            }
                                        }

                                        if (dbOps[i].table.RowUpdatedCallback != null)
                                        {
                                            try
                                            {
                                                dbOps[i].table.RowUpdatedCallback
                                                    .Invoke(tableOp, oldValue, null, message.TransactionUpdate?.Event);
                                            }
                                            catch (Exception e)
                                            {
                                                logger.LogException(e);
                                            }
                                        }
                                    }
                                    else
                                    {
                                        logger.LogError("Failed to send callback: invalid delete");
                                    }

                                    break;
                                }
                            case TableOp.Update:
                                {
                                    if (oldValue != null && newValue != null)
                                    {
                                        try
                                        {
                                            if (dbOps[i].table.UpdateCallback != null)
                                            {
                                                dbOps[i].table.UpdateCallback.Invoke(oldValue, newValue,
                                                    message.TransactionUpdate?.Event);
                                            }
                                        }
                                        catch (Exception e)
                                        {
                                            logger.LogException(e);
                                        }

                                        try
                                        {
                                            if (dbOps[i].table.RowUpdatedCallback != null)
                                            {
                                                dbOps[i].table.RowUpdatedCallback
                                                    .Invoke(tableOp, oldValue, newValue, message.TransactionUpdate?.Event);
                                            }
                                        }
                                        catch (Exception e)
                                        {
                                            logger.LogException(e);
                                        }
                                    }
                                    else
                                    {
                                        logger.LogError("Failed to send callback: invalid update");
                                    }

                                    break;
                                }
                            case TableOp.NoChange:
                                // noop
                                break;
                            default:
                                throw new ArgumentOutOfRangeException();
                        }

                        if (tableOp != TableOp.NoChange)
                        {
                            onRowUpdate?.Invoke(tableName, tableOp, oldValue, newValue,
                                message.Event?.FunctionCall.CallInfo);
                        }
                    }

                    switch (message.TypeCase)
                    {
                        case Message.TypeOneofCase.SubscriptionUpdate:
                            try
                            {
                                onSubscriptionApplied?.Invoke();
                            }
                            catch (Exception e)
                            {
                                logger.LogException(e);
                            }

                            break;
                        case Message.TypeOneofCase.TransactionUpdate:
                            try
                            {
                                onEvent?.Invoke(message.TransactionUpdate.Event);
                            }
                            catch (Exception e)
                            {
                                logger.LogException(e);
                            }

                            bool reducerFound = false;
                            var functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                            if (reducerEventCache.TryGetValue(functionName, out var value))
                            {
                                try
                                {
                                    reducerFound = value.Invoke(message.TransactionUpdate.Event);
                                }
                                catch (Exception e)
                                {
                                    logger.LogException(e);
                                }
                            }

                            if (!reducerFound && message.TransactionUpdate.Event.Status ==
                                ClientApi.Event.Types.Status.Failed)
                            {
                                try
                                {
                                    onUnhandledReducerError?.Invoke(message.TransactionUpdate.Event.FunctionCall
                                        .CallInfo);
                                }
                                catch (Exception e)
                                {
                                    logger.LogException(e);
                                }
                            }

                            break;
                        case Message.TypeOneofCase.None:
                            break;
                        case Message.TypeOneofCase.FunctionCall:
                            break;
                        case Message.TypeOneofCase.Event:
                            break;
                        case Message.TypeOneofCase.IdentityToken:
                            break;
                        default:
                            throw new ArgumentOutOfRangeException();
                    }

                    break;
                case Message.TypeOneofCase.IdentityToken:
                    try
                    {
                        onIdentityReceived?.Invoke(message.IdentityToken.Token,
                            Identity.From(message.IdentityToken.Identity.ToByteArray()),
                            (Address)Address.From(message.IdentityToken.Address.ToByteArray()));
                    }
                    catch (Exception e)
                    {
                        logger.LogException(e);
                    }

                    break;
                case Message.TypeOneofCase.Event:
                    try
                    {
                        onEvent?.Invoke(message.Event);
                    }
                    catch (Exception e)
                    {
                        logger.LogException(e);
                    }

                    break;
            }
        }

        private void OnMessageReceived(byte[] bytes) => _messageQueue.Add(bytes);

        public void InternalCallReducer(string json)
        {
            if (!webSocket.IsConnected)
            {
                logger.LogError("Cannot call reducer, not connected to server!");
                return;
            }

            webSocket.Send(Encoding.ASCII.GetBytes("{ \"call\": " + json + " }"));
        }

        public void Subscribe(List<string> queries)
        {
            if (!webSocket.IsConnected)
            {
                logger.LogError("Cannot subscribe, not connected to server!");
                return;
            }

            var json = JsonConvert.SerializeObject(queries);
            // should we use UTF8 here? ASCII is fragile.
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"subscribe\": { \"query_strings\": " + json + " }}"));
        }

        /// Usage: SpacetimeDBClient.instance.OneOffQuery<Message>("WHERE sender = \"bob\"");
        public async Task<T[]> OneOffQuery<T>(string query) where T : IDatabaseTable
        {
            Guid messageId = Guid.NewGuid();
            Type type = typeof(T);
            Channel<OneOffQueryResponse> resultChannel = Channel.CreateBounded<OneOffQueryResponse>(1);
            waitingOneOffQueries[messageId] = resultChannel;

            // unsanitized here, but writes will be prevented serverside.
            // the best they can do is send multiple selects, which will just result in them getting no data back.
            string queryString = "SELECT * FROM " + type.Name + " " + query;

            // see: SpacetimeDB\crates\core\src\client\message_handlers.rs, enum Message<'a>
            var serializedQuery = "{ \"one_off_query\": { \"message_id\": \"" +
                                  System.Convert.ToBase64String(messageId.ToByteArray()) +
                                  "\", \"query_string\": " + JsonConvert.SerializeObject(queryString) + " } }";
            webSocket.Send(Encoding.UTF8.GetBytes(serializedQuery));

            // Suspend for an arbitrary amount of time
            var result = await resultChannel.Reader.ReadAsync();

            T[] LogAndThrow(string error)
            {
                error = "While processing one-off-query `" + queryString + "`, ID " + messageId + ": " + error;
                logger.LogError(error);
                throw new Exception(error);
            }

            // The server got back to us
            if (result.Error != null && result.Error != "")
            {
                return LogAndThrow("Server error: " + result.Error);
            }

            if (result.Tables.Count != 1)
            {
                return LogAndThrow("Expected a single table, but got " + result.Tables.Count);
            }

            var resultTable = result.Tables[0];
            var cacheTable = clientDB.GetTable(resultTable.TableName);

            if (cacheTable.ClientTableType != type)
            {
                return LogAndThrow("Mismatched result type, expected " + type + " but got " + resultTable.TableName);
            }

            T[] results = (T[])Array.CreateInstance(type, resultTable.Row.Count);
            using var stream = new MemoryStream();
            using var reader = new BinaryReader(stream);
            for (int i = 0; i < results.Length; i++)
            {
                var rowValue = resultTable.Row[i].ToByteArray();
                stream.Position = 0;
                stream.Write(rowValue, 0, rowValue.Length);
                stream.Position = 0;
                stream.SetLength(rowValue.Length);

                var deserialized = AlgebraicValue.Deserialize(cacheTable.RowSchema, reader);
                cacheTable.SetAndForgetDecodedValue(deserialized, out var obj);
                results[i] = (T)obj;
            }

            return results;
        }

        public bool IsConnected() => webSocket != null && webSocket.IsConnected;

        public void Update()
        {
            webSocket.Update();
            while (_stateDiffMessages.TryTake(out var stateDiffMessage))
            {
                OnMessageProcessComplete(stateDiffMessage.message, stateDiffMessage.dbOps);
            }
        }
    }
}
