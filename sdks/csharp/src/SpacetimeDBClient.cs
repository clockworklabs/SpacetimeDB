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

namespace SpacetimeDB
{
    public class SpacetimeDBClient
    {
        public enum TableOp
        {
            Insert,
            Delete,
            Update
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
            public byte[] deletedPk;
            public byte[] insertedPk;
            public AlgebraicValue primaryKeyValue;
        }

        public delegate void RowUpdate(string tableName, TableOp op, object oldValue, object newValue, SpacetimeDB.ReducerEventBase dbEvent);

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?, string?> onConnectError;

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
        public static Dictionary<string, Func<ClientApi.Event, bool>> reducerEventCache = new Dictionary<string, Func<ClientApi.Event, bool>>();
        public static Dictionary<string, Action<ClientApi.Event>> deserializeEventCache = new Dictionary<string, Action<ClientApi.Event>>();

        private static Dictionary<Guid, Channel<OneOffQueryResponse>> waitingOneOffQueries = new Dictionary<Guid, Channel<OneOffQueryResponse>>();

        private bool isClosing;
        private Thread messageProcessThread;

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
                        reducerEventCache.Add(reducerEvent.FunctionName, (Func<ClientApi.Event, bool>)methodInfo.CreateDelegate(typeof(Func<ClientApi.Event, bool>)));
                    }

                    if (methodInfo.GetCustomAttribute<DeserializeEventAttribute>() is
                        { } deserializeEvent)
                    {
                        deserializeEventCache.Add(deserializeEvent.FunctionName, (Action<ClientApi.Event>)methodInfo.CreateDelegate(typeof(Action<ClientApi.Event>)));
                    }
                }
            }
            else
            {
                loggerToUse.LogError($"Could not find reducer type. Have you run spacetime generate?");
            }

            _cancellationToken = _cancellationTokenSource.Token;
            messageProcessThread = new Thread(ProcessMessages);
            messageProcessThread.Start();
        }

        struct ProcessedMessage
        {
            public Message message;
            public IList<DbOp> dbOps;
        }

        private readonly BlockingCollection<byte[]> _messageQueue = new BlockingCollection<byte[]>(new ConcurrentQueue<byte[]>());
        private readonly BlockingCollection<ProcessedMessage> _nextMessageQueue = new BlockingCollection<ProcessedMessage>(new ConcurrentQueue<ProcessedMessage>());

        CancellationTokenSource _cancellationTokenSource = new CancellationTokenSource();
        CancellationToken _cancellationToken;

        void ProcessMessages()
        {
            while (!isClosing)
            {
                try
                {
                    var bytes = _messageQueue.Take(_cancellationToken);

                    var (m, events) = PreProcessMessage(bytes);
                    var processedMessage = new ProcessedMessage
                    {
                        message = m,
                        dbOps = events,
                    };
                    _nextMessageQueue.Add(processedMessage);
                }
                catch (OperationCanceledException)
                {
                    // Normal shutdown
                    return;
                }
            }

            (Message, List<DbOp>) PreProcessMessage(byte[] bytes)
            {
                var dbOps = new List<DbOp>();

                // Used when convering matching Insert/Delete ops into Update
                var insertOps = new List<DbOp>();

                var message = ClientApi.Message.Parser.ParseFrom(bytes);
                using var stream = new MemoryStream();
                using var reader = new BinaryReader(stream);

                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        break;
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        break;
                }

                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
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

                            var primaryKeyType = table.GetPrimaryKeyType(table.RowSchema);
                            var deleteOps = new Dictionary<AlgebraicValue, DbOp>(new AlgebraicValue.AlgebraicValueComparer(primaryKeyType));
                            insertOps.Clear();
                            foreach (var row in update.TableRowOperations)
                            {
                                var rowPk = row.RowPk.ToByteArray();
                                var rowValue = row.Row.ToByteArray();
                                stream.Position = 0;
                                stream.Write(rowValue, 0, rowValue.Length);
                                stream.Position = 0;
                                stream.SetLength(rowValue.Length);
                                var decodedRow = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                if (decodedRow == null)
                                {
                                    throw new Exception("Failed to deserialize row");
                                }

                                DbOp dbOp;
                                AlgebraicValue primaryKeyValue = table.GetPrimaryKeyValue(decodedRow);
                                if (row.Op == TableRowOperation.Types.OperationType.Delete)
                                {
                                    dbOp = new DbOp
                                    {
                                        table = table,
                                        deletedPk = rowPk,
                                        op = TableOp.Delete,
                                        newValue = null,
                                        // We cannot grab the old value here because there might be other
                                        // pending operations that will execute before us. We should only
                                        // set this value on the main thread where we know there are no other
                                        // operations which could remove this value.
                                        oldValue = null,
                                        primaryKeyValue = primaryKeyValue
                                    };
                                }
                                else
                                {
                                    // Skip this insert if we already have it
                                    if (table.entries.ContainsKey(rowPk))
                                    {
                                        continue;
                                    }

                                    table.SetDecodedValue(rowPk, decodedRow, out var obj);
                                    dbOp = new DbOp
                                    {
                                        table = table,
                                        insertedPk = rowPk,
                                        op = TableOp.Insert,
                                        newValue = obj,
                                        oldValue = null,
                                        primaryKeyValue = primaryKeyValue
                                    };
                                }

                                if (primaryKeyType != null)
                                {
                                    if (row.Op == TableRowOperation.Types.OperationType.Delete)
                                    {
                                        deleteOps[primaryKeyValue] = dbOp;
                                    }
                                    else
                                    {
                                        insertOps.Add(dbOp);
                                    }
                                }
                                else
                                {
                                    dbOps.Add(dbOp);
                                }
                            }

                            if (primaryKeyType != null)
                            {
                                // Replace Delete/Insert pairs with identical primary keys with an Update.
                                //
                                // !!TODO!!: Currently this code interprets Insert/Delete or Delete/Insert pairs as Updates.
                                // Note that if a user inserts and then deletes a row with the same primary key in a
                                // spacetimedb module, this is interpreted as an Update, while effectively it is a NoOp.
                                for (var i = 0; i < insertOps.Count; i++)
                                {
                                    var insertOp = insertOps[i];
                                    if (deleteOps.TryGetValue(insertOp.primaryKeyValue, out var deleteOp))
                                    {
                                        // We found an insert with a matching delete.
                                        // Replace it with an update operation.
                                        insertOps[i] = new DbOp
                                        {
                                            insertedPk = insertOp.insertedPk,
                                            deletedPk = deleteOp.deletedPk,
                                            table = insertOp.table,
                                            op = TableOp.Update
                                        };
                                        deleteOps.Remove(insertOp.primaryKeyValue);
                                    }
                                }
                                dbOps.AddRange(insertOps);
                                dbOps.AddRange(deleteOps.Values);
                            }
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

                if (message.TypeCase == Message.TypeOneofCase.SubscriptionUpdate)
                {
                    // NOTE: We are going to calculate a local state diff here. This is kind of expensive to do,
                    // but keep in mind that we're still on the network thread so spending some extra time here
                    // is acceptable.
                    if (subscriptionUpdate!.TableUpdates.Any(
                            a => a.TableRowOperations.Any(b => b.Op == TableRowOperation.Types.OperationType.Delete)))
                    {
                        logger.LogWarning(
                            "We see delete events in our subscription update, this is unexpected. Likely you should update your SpacetimeDBUnitySDK.");
                    }

                    foreach (var tableUpdate in subscriptionUpdate.TableUpdates)
                    {
                        var clientTable = clientDB.GetTable(tableUpdate.TableName);
                        var newPks = tableUpdate.TableRowOperations
                                                .Where(a => a.Op == TableRowOperation.Types.OperationType.Insert)
                                                .Select(b => b.RowPk.ToByteArray());
                        var existingPks = clientTable.entries.Select(a => a.Key);
                        dbOps.AddRange(existingPks.Except(newPks, new ClientCache.TableCache.ByteArrayComparer())
                        .Select(a => new DbOp
                        {
                            deletedPk = a,
                            newValue = null,
                            oldValue = clientTable.entries[a].Item2,
                            op = TableOp.Delete,
                            table = clientTable,
                        }));
                    }
                }

                if (message.TypeCase == Message.TypeOneofCase.TransactionUpdate &&
                    deserializeEventCache.TryGetValue(message.TransactionUpdate.Event.FunctionCall.Reducer, out var deserializer))
                {
                    deserializer.Invoke(message.TransactionUpdate.Event);
                }

                return (message, dbOps);
            }
        }

        public void Close()
        {
            isClosing = true;
            connectionClosed = true;
            webSocket.Close();
            _cancellationTokenSource.Cancel();
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

        private void OnMessageProcessComplete(Message message, IList<DbOp> dbOps)
        {
            switch (message.TypeCase)
            {
                case Message.TypeOneofCase.SubscriptionUpdate:
                case Message.TypeOneofCase.TransactionUpdate:
                    // First trigger OnBeforeDelete
                    for (var i = 0; i < dbOps.Count; i++)
                    {
                        // TODO: Reimplement updates when we add support for primary keys
                        var update = dbOps[i];
                        if (update.op == TableOp.Delete && update.table.TryGetValue(update.deletedPk, out var oldVal))
                        {
                            try
                            {
                                update.table.BeforeDeleteCallback?.Invoke(oldVal, message.TransactionUpdate?.Event);
                            }
                            catch (Exception e)
                            {
                                logger.LogException(e);
                            }
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
                                update.oldValue = dbOps[i].table.DeleteEntry(update.deletedPk);
                                if (update.oldValue != null)
                                {
                                    update.table.InternalValueDeletedCallback(update.oldValue);
                                }
                                dbOps[i] = update;
                                break;
                            case TableOp.Insert:
                                update.newValue = dbOps[i].table.InsertEntry(update.insertedPk);
                                update.table.InternalValueInsertedCallback(update.newValue);
                                dbOps[i] = update;
                                break;
                            case TableOp.Update:
                                update.oldValue = dbOps[i].table.DeleteEntry(update.deletedPk);
                                update.newValue = dbOps[i].table.InsertEntry(update.insertedPk);
                                if (update.oldValue != null)
                                {
                                    update.table.InternalValueDeletedCallback(update.oldValue);
                                }
                                update.table.InternalValueInsertedCallback(update.newValue);
                                dbOps[i] = update;
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
                                            dbOps[i].table.InsertCallback.Invoke(newValue, message.TransactionUpdate?.Event);
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
                                                dbOps[i].table.DeleteCallback.Invoke(oldValue, message.TransactionUpdate?.Event);
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
                                                dbOps[i].table.UpdateCallback.Invoke(oldValue, newValue, message.TransactionUpdate?.Event);
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
                                                        .Invoke(tableOp, oldValue, null, message.TransactionUpdate?.Event);
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
                            default:
                                throw new ArgumentOutOfRangeException();
                        }

                        onRowUpdate?.Invoke(tableName, tableOp, oldValue, newValue, message.Event?.FunctionCall.CallInfo);
                    }

                    switch (message.TypeCase)
                    {
                        case Message.TypeOneofCase.SubscriptionUpdate:
                            onSubscriptionApplied?.Invoke();
                            break;
                        case Message.TypeOneofCase.TransactionUpdate:
                            onEvent?.Invoke(message.TransactionUpdate.Event);

                            bool reducerFound = false;
                            var functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                            if (reducerEventCache.TryGetValue(functionName, out var value))
                            {
                                reducerFound = value.Invoke(message.TransactionUpdate.Event);
                            }
                            if (!reducerFound && message.TransactionUpdate.Event.Status == ClientApi.Event.Types.Status.Failed)
                            {
                                onUnhandledReducerError?.Invoke(message.TransactionUpdate.Event.FunctionCall.CallInfo);
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
                  onIdentityReceived?.Invoke(message.IdentityToken.Token, Identity.From(message.IdentityToken.Identity.ToByteArray()), (Address)Address.From(message.IdentityToken.Address.ToByteArray()));
                    break;
                case Message.TypeOneofCase.Event:
                    onEvent?.Invoke(message.Event);
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
            var serializedQuery = "{ \"one_off_query\": { \"message_id\": \"" + System.Convert.ToBase64String(messageId.ToByteArray()) +
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

        public void Update()
        {
            webSocket.Update();

            while (_nextMessageQueue.TryTake(out var nextMessage))
            {
                OnMessageProcessComplete(nextMessage.message, nextMessage.dbOps);
            }
        }
    }
}
